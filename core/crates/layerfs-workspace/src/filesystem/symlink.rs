//! Native symbolic links retain their opaque target in the existing payload owner.
use super::create::Creation;
use crate::{
    backing::{metadata::RootOwner, metadata_index::vector},
    overlay::pieces::{Inode, PieceKind},
    runtime::coherence::MutationOrigin,
    *,
};
use layerfs_bridge::contract::{Inspect, Operation, Response, Source, SYMLINK_TARGET_BYTES};
use std::{sync::Arc, time::Instant};

impl Workspace {
    /// Creates a symlink with mode 0777 and one Local lookup reference, without
    /// opening a handle. Targets are opaque non-NUL bytes, including empty.
    /// Mounted success includes checked parent/entry invalidation. A later
    /// Coherence error retains the published name and target and releases only
    /// this attempt's unreturned lookup reference.
    pub fn symlink(
        &self,
        parent: u64,
        name: &[u8],
        target: &[u8],
        deadline: Instant,
    ) -> Result<NodeAttributes, WorkspaceError> {
        self.symlink_from(parent, name, target, deadline, MutationOrigin::Local)
    }
    pub(crate) fn symlink_from(
        &self,
        parent: u64,
        name: &[u8],
        target: &[u8],
        deadline: Instant,
        origin: MutationOrigin,
    ) -> Result<NodeAttributes, WorkspaceError> {
        self.create_child(parent, name, Creation::Symlink { target, origin }, deadline)
            .map(|(attributes, _)| attributes)
    }

    /// Reads one selected local or canonical target without following the link.
    pub fn readlink(&self, serial: u64, deadline: Instant) -> Result<ReadReply, WorkspaceError> {
        let deadline = Self::callback_deadline(deadline);
        let mut operation = self.begin(false, deadline)?;
        operation.local_io()?;
        let (size, base, root, active) = {
            let state = self.state()?;
            let node = state.node(serial)?;
            if node.attr.kind != NodeKind::Symlink {
                return Err(WorkspaceError::WrongKind);
            }
            let active = if self.inner.active.is_some() {
                let backing = self
                    .inner
                    .active
                    .as_ref()
                    .ok_or(WorkspaceError::Unsupported)?;
                Some((backing.clone(), backing.pin_view()?))
            } else {
                None
            };
            (node.attr.size, state.base, state.overlay.clone(), active)
        };
        if size > SYMLINK_TARGET_BYTES as u64 {
            return Err(WorkspaceError::Io);
        }
        let mut charge = self.host.budget.reserve(SYMLINK_TARGET_BYTES)?;
        let bytes = if let Some((backing, snapshot)) = &active {
            if snapshot
                .get(&crate::backing::active::inode_key(serial))?
                .is_some()
            {
                let mut bytes = vec![0; size as usize];
                let count = backing.read_file(
                    serial,
                    0,
                    &mut bytes,
                    Some(snapshot),
                    |_, _, _| Err(WorkspaceError::Io),
                    |payload, start, output| {
                        let mut reader = payload.reader(start..start + output.len() as u64)?;
                        let mut done = 0;
                        while done < output.len() {
                            let count = Source::read(
                                &mut reader,
                                &mut output[done..],
                                deadline,
                                &self.inner.stopping,
                            )
                            .map_err(|_| WorkspaceError::Io)?;
                            if count == 0 {
                                return Err(WorkspaceError::Io);
                            }
                            done += count;
                        }
                        Ok(())
                    },
                )?;
                if count != bytes.len() {
                    return Err(WorkspaceError::Io);
                }
                bytes
            } else {
                self.readlink_inode_canonical(&mut operation, base, serial, deadline)?
            }
        } else if let Some(inode) = self.overlay_inode(serial, root.as_ref(), deadline)? {
            self.symlink_target(root.as_ref().ok_or(WorkspaceError::Io)?, inode, deadline)?
        } else {
            self.readlink_inode_canonical(&mut operation, base, serial, deadline)?
        };
        if bytes.len() > SYMLINK_TARGET_BYTES || bytes.len() as u64 != size || bytes.contains(&0) {
            return Err(WorkspaceError::InvalidInput);
        }
        charge.resize(bytes.capacity())?;
        Ok(ReadReply {
            bytes,
            _charge: charge,
            _operation: operation,
        })
    }

    pub(super) fn readlink_inode_canonical(
        &self,
        operation: &mut crate::runtime::state::OperationGuard,
        base: [u8; 32],
        serial: u64,
        deadline: Instant,
    ) -> Result<Vec<u8>, WorkspaceError> {
        self.readlink_inspect(operation, base, Inspect::InodeReadlink { serial }, deadline)
    }

    fn readlink_inspect(
        &self,
        operation: &mut crate::runtime::state::OperationGuard,
        base: [u8; 32],
        query: Inspect,
        deadline: Instant,
    ) -> Result<Vec<u8>, WorkspaceError> {
        operation.remote()?;
        let response = self.call(
            Operation::Inspect { root: base, query },
            0,
            &mut std::io::sink(),
            deadline,
        );
        operation.release_remote();
        match response? {
            Response::Link(bytes) => Ok(bytes),
            _ => Err(WorkspaceError::InvalidInput),
        }
    }

    /// The caller holds its existing I/O allowance while materializing this bounded target.
    pub(crate) fn symlink_target(
        &self,
        root: &Arc<RootOwner>,
        inode: Inode,
        deadline: Instant,
    ) -> Result<Vec<u8>, WorkspaceError> {
        if !inode.symlink || inode.length > SYMLINK_TARGET_BYTES as u64 {
            return Err(WorkspaceError::Io);
        }
        let mut bytes = vector(inode.length as usize)?;
        bytes.resize(inode.length as usize, 0);
        if bytes.is_empty() {
            return Ok(bytes);
        }
        let piece = {
            let host = self
                .host
                .metadata
                .as_ref()
                .ok_or(WorkspaceError::Unsupported)?;
            let _view = host.writer()?;
            let mut lease = host.payloads.window(1, 3)?;
            let (start, piece) = root.arena.piece_at(
                inode.pieces,
                0,
                inode.length,
                lease.window.as_mut().ok_or(WorkspaceError::Io)?,
                deadline,
            )?;
            if start != 0 {
                return Err(WorkspaceError::Io);
            }
            piece
        };
        if piece.kind != PieceKind::Local || piece.offset != 0 || piece.length != inode.length {
            return Err(WorkspaceError::Io);
        }
        let payload = root.arena.payload(piece.payload, piece.custody)?;
        if payload.len() != inode.length {
            return Err(WorkspaceError::Io);
        }
        let mut reader = payload.reader(0..inode.length)?;
        let mut received = 0;
        while received < bytes.len() {
            let n = Source::read(
                &mut reader,
                &mut bytes[received..],
                deadline,
                &self.inner.stopping,
            )
            .map_err(|error| {
                error
                    .get_ref()
                    .and_then(|inner| inner.downcast_ref::<BackingFailure>())
                    .map_or(WorkspaceError::Io, |failure| {
                        WorkspaceError::Backing(failure.clone())
                    })
            })?;
            if n == 0 {
                return Err(WorkspaceError::Io);
            }
            received += n;
        }
        if bytes.contains(&0) {
            return Err(WorkspaceError::Io);
        }
        Ok(bytes)
    }
}
