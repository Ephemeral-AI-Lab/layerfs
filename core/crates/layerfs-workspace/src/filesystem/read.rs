use crate::*;
use layerfs_bridge::contract::{Inspect, Operation, Response};
use std::{io::Cursor, time::Instant};

impl Workspace {
    pub fn open(&self, serial: u64, scope: ReferenceScope) -> Result<HandleId, WorkspaceError> {
        self.open_file(
            serial,
            FileOpenOptions::default(),
            scope,
            Instant::now() + std::time::Duration::from_secs(10),
        )
    }
    pub fn read(
        &self,
        handle: HandleId,
        offset: u64,
        size: usize,
        deadline: Instant,
    ) -> Result<ReadReply, WorkspaceError> {
        if size > MAX_READ_BYTES {
            return Err(WorkspaceError::Capacity);
        }
        let deadline = Self::callback_deadline(deadline);
        let _path_charge = self
            .host
            .budget
            .reserve(crate::runtime::state::PATH_BYTES)?;
        let (attr, mut content, root, base, baseline, path, path_len, stale) = {
            let state = self.state()?;
            self.available(&state)?;
            let handle = state.handle(handle, false)?;
            if handle.options.access == FileAccess::WriteOnly {
                return Err(WorkspaceError::BadHandle);
            }
            let node = state.node(handle.serial)?;
            (
                node.attr,
                node.content,
                state.overlay.clone(),
                state.base,
                state.baseline,
                node.path,
                node.path_len,
                node.baseline != state.baseline,
            )
        };
        let length = if offset >= attr.size {
            0
        } else {
            (attr.size - offset).min(size as u64) as usize
        };
        let mut operation = self.begin(false, deadline)?;
        let charge = self.host.budget.reserve(length)?;
        let mut bytes = Vec::new();
        bytes
            .try_reserve_exact(length)
            .map_err(|_| WorkspaceError::Capacity)?;
        bytes.resize(length, 0);
        if length > 0 {
            if let Some(inode) = self.overlay_inode(attr.serial, root.as_ref(), deadline)? {
                self.read_overlay(
                    root.as_ref().ok_or(WorkspaceError::Io)?,
                    inode,
                    offset,
                    &mut bytes,
                    &mut operation,
                    deadline,
                )?;
            } else {
                operation.remote()?;
                if stale {
                    let mut selected_path = crate::backing::metadata_index::vector(path_len)?;
                    selected_path.extend_from_slice(&path[..path_len]);
                    let response = self.call(
                        Operation::Inspect {
                            root: base,
                            query: Inspect::Attributes {
                                path: selected_path,
                            },
                        },
                        0,
                        &mut std::io::sink(),
                        deadline,
                    )?;
                    let (original, canonical, metadata) = super::namespace::attributes(
                        response,
                        false,
                        self.inner.root.uid,
                        self.inner.root.gid,
                    )?;
                    if original != attr {
                        return Err(WorkspaceError::InvalidInput);
                    }
                    content = canonical;
                    let mut state = self.state()?;
                    if state.baseline == baseline {
                        let node = state.node_mut(attr.serial)?;
                        node.original = original;
                        node.content = canonical;
                        node.metadata = metadata;
                        node.baseline = baseline;
                    }
                }
                let mut output = Cursor::new(bytes.as_mut_slice());
                let response = self.call(
                    Operation::ReadFile {
                        root: content,
                        start: offset,
                        end: offset + length as u64,
                    },
                    length as u64,
                    &mut output,
                    deadline,
                )?;
                if response
                    != (Response::Read {
                        length: length as u64,
                    })
                    || output.position() != length as u64
                {
                    return Err(WorkspaceError::InvalidInput);
                }
            }
        }
        Ok(ReadReply {
            bytes,
            _charge: charge,
            _operation: operation,
        })
    }
    /// Reports a retained notification failure for this inode, without saving
    /// or releasing any ownership. Handle release remains a separate operation.
    pub fn flush(&self, handle: HandleId) -> Result<(), WorkspaceError> {
        let state = self.state()?;
        let handle = state.handle(handle, false)?;
        if let Some(CoherenceStatus::Failed(failure)) = state.projection.as_ref().map(|p| p.status)
        {
            if failure.receipt.inode == handle.serial {
                return Err(WorkspaceError::Coherence(failure));
            }
        }
        Ok(())
    }
    pub fn release(&self, handle: HandleId) -> Result<(), WorkspaceError> {
        self.release_handle(handle, false)
    }
}

impl Workspace {
    fn read_overlay(
        &self,
        root: &std::sync::Arc<crate::backing::metadata::RootOwner>,
        inode: crate::overlay::pieces::Inode,
        offset: u64,
        bytes: &mut [u8],
        operation: &mut crate::runtime::state::OperationGuard,
        deadline: Instant,
    ) -> Result<(), WorkspaceError> {
        use layerfs_bridge::contract::Source;
        if inode.symlink {
            return Err(WorkspaceError::Io);
        }
        let host = self
            .host
            .payloads
            .as_ref()
            .ok_or(WorkspaceError::Unsupported)?;
        let mut completed = 0;
        while completed < bytes.len() {
            let position = offset + completed as u64;
            // One bounded lookup per step: the cursor seeks to the extent that
            // covers this position, reads only the pages on its path, and
            // derives the extent's logical start from those pages.
            let (start, piece) = {
                let _view = self
                    .host
                    .metadata
                    .as_ref()
                    .ok_or(WorkspaceError::Unsupported)?
                    .writer()?;
                let mut lease = host.window(1, 3)?;
                root.arena.piece_at(
                    inode.pieces,
                    position,
                    inode.length,
                    lease.window.as_mut().ok_or(WorkspaceError::Io)?,
                    deadline,
                )?
            };
            let skip = position - start;
            let count = (piece.length - skip).min((bytes.len() - completed) as u64) as usize;
            if piece.kind == crate::overlay::pieces::PieceKind::Zero {
                bytes[completed..completed + count].fill(0);
            } else if piece.kind == crate::overlay::pieces::PieceKind::Base && inode.captured {
                let captured = crate::overlay::pieces::CapturedBase::parse(inode.base)?;
                let parent = root.parent.as_ref().ok_or(WorkspaceError::Io)?;
                if parent.root()? != captured.root {
                    return Err(WorkspaceError::Io);
                }
                let inherited = self
                    .overlay_inode(captured.inode, Some(parent), deadline)?
                    .ok_or(WorkspaceError::Io)?;
                if inherited.captured
                    || inherited.generation != captured.generation
                    || inherited.revision != captured.revision
                    || inherited.length != inode.base_length
                {
                    return Err(WorkspaceError::Io);
                }
                self.read_overlay(
                    parent,
                    inherited,
                    piece.offset + skip,
                    &mut bytes[completed..completed + count],
                    operation,
                    deadline,
                )?;
            } else if piece.kind == crate::overlay::pieces::PieceKind::Base {
                operation.remote()?;
                let mut output = Cursor::new(&mut bytes[completed..completed + count]);
                let response = self.call(
                    Operation::ReadFile {
                        root: inode.base,
                        start: piece.offset + skip,
                        end: piece.offset + skip + count as u64,
                    },
                    count as u64,
                    &mut output,
                    deadline,
                )?;
                if response
                    != (Response::Read {
                        length: count as u64,
                    })
                    || output.position() != count as u64
                {
                    return Err(WorkspaceError::InvalidInput);
                }
            } else {
                let payload = root.arena.payload(piece.payload, piece.custody)?;
                let mut reader =
                    payload.reader(piece.offset + skip..piece.offset + skip + count as u64)?;
                let mut received = 0;
                while received < count {
                    let n = Source::read(
                        &mut reader,
                        &mut bytes[completed + received..completed + count],
                        deadline,
                        &self.inner.stopping,
                    )?;
                    if n == 0 {
                        return Err(WorkspaceError::Io);
                    }
                    received += n;
                }
            }
            completed += count;
        }
        Ok(())
    }
}
