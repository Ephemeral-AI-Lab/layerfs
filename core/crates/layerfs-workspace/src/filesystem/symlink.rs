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
        let (path, size, base, root) = {
            let state = self.state()?;
            let node = state.node(serial)?;
            if node.attr.kind != NodeKind::Symlink {
                return Err(WorkspaceError::WrongKind);
            }
            (
                node.path().to_vec(),
                node.attr.size,
                state.base,
                state.overlay.clone(),
            )
        };
        if size > SYMLINK_TARGET_BYTES as u64 {
            return Err(WorkspaceError::Io);
        }
        let mut charge = self.host.budget.reserve(SYMLINK_TARGET_BYTES)?;
        let bytes = if let Some(inode) = self.overlay_inode(serial, root.as_ref(), deadline)? {
            self.symlink_target(root.as_ref().ok_or(WorkspaceError::Io)?, inode, deadline)?
        } else {
            operation.remote()?;
            let response = self.call(
                Operation::Inspect {
                    root: base,
                    query: Inspect::Readlink { path },
                },
                0,
                &mut std::io::sink(),
                deadline,
            );
            operation.release_remote();
            let Response::Link(bytes) = response? else {
                return Err(WorkspaceError::InvalidInput);
            };
            bytes
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
            let pieces = root.arena.pieces(
                inode.pieces,
                inode.count,
                inode.length,
                lease.window.as_mut().ok_or(WorkspaceError::Io)?,
                deadline,
            )?;
            if pieces.len() != 1 {
                return Err(WorkspaceError::Io);
            }
            pieces[0]
        };
        if piece.kind != PieceKind::Local
            || piece.start != 0
            || piece.offset != 0
            || piece.length != inode.length
        {
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
