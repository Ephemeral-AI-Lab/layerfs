use crate::*;
use layerfs_bridge::contract::{Inspect, Operation, Response};
use std::{io::Cursor, time::Instant};

impl Workspace {
    pub fn open(&self, serial: u64, scope: ReferenceScope) -> Result<HandleId, WorkspaceError> {
        self.open_handle(serial, false, scope)
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
        let (attr, content, root) = {
            let state = self.state()?;
            self.available(&state)?;
            let handle = state.handle(handle, false)?;
            let node = state.node(handle.serial)?;
            (node.attr, node.content, state.overlay.clone())
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
    pub fn readlink(&self, serial: u64, deadline: Instant) -> Result<ReadReply, WorkspaceError> {
        let deadline = Self::callback_deadline(deadline);
        let operation = self.begin(true, deadline)?;
        let (path, size) = {
            let state = self.state()?;
            let node = state.node(serial)?;
            if node.attr.kind != NodeKind::Symlink {
                return Err(WorkspaceError::WrongKind);
            }
            (node.path().to_vec(), node.attr.size)
        };
        let response = self.call(
            Operation::Inspect {
                root: self.inner.base,
                query: Inspect::Readlink { path },
            },
            0,
            &mut std::io::sink(),
            deadline,
        )?;
        let Response::Link(bytes) = response else {
            return Err(WorkspaceError::InvalidInput);
        };
        if bytes.len() > 4096 || bytes.len() as u64 != size || bytes.contains(&0) {
            return Err(WorkspaceError::InvalidInput);
        }
        let charge = self.host.budget.reserve(bytes.capacity())?;
        Ok(ReadReply {
            bytes,
            _charge: charge,
            _operation: operation,
        })
    }
    pub fn flush(&self, handle: HandleId) -> Result<(), WorkspaceError> {
        self.state()?.handle(handle, false)?;
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
        let host = self
            .host
            .payloads
            .as_ref()
            .ok_or(WorkspaceError::Unsupported)?;
        let mut completed = 0;
        while completed < bytes.len() {
            let position = offset + completed as u64;
            let piece = {
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
                    lease.window.as_mut().ok_or(WorkspaceError::Io)?,
                    deadline,
                )?
            };
            let skip = position - piece.start;
            let count = (piece.length - skip).min((bytes.len() - completed) as u64) as usize;
            if piece.payload == 0 && inode.captured {
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
            } else if piece.payload == 0 {
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
