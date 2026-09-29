//! Original facts for a live file; unresolved local identities never query absent B paths.
use super::namespace::attributes;
use crate::{runtime::state::OperationGuard, *};
use layerfs_bridge::contract::{Inspect, Root};
use std::time::Instant;

pub(super) type Original = (NodeAttributes, Root, Root, u64);

impl Workspace {
    pub(super) fn serial_original(
        &self,
        serial: u64,
        deadline: Instant,
        operation: &mut OperationGuard,
    ) -> Result<Original, WorkspaceError> {
        let (base, root, baseline, selected) = {
            let state = self.state()?;
            self.available(&state)?;
            let node = state.node(serial)?;
            if node.attr.kind == NodeKind::Directory {
                return Err(WorkspaceError::IsDirectory);
            }
            if node.attr.kind != NodeKind::File {
                return Err(WorkspaceError::WrongKind);
            }
            if let Some(active) = &self.inner.active {
                // I.base is a file root; State.base is a filesystem root.
                // A later canonical Commit must never reinterpret G2 Base offsets.
                if let Some(inode) = active.file_facts(serial)?.0 {
                    return Ok((node.attr, inode.base, inode.metadata, state.baseline));
                }
            }
            if node.baseline == state.baseline {
                return Ok((node.original, node.content, node.metadata, state.baseline));
            }
            (state.base, state.overlay.clone(), state.baseline, node.attr)
        };
        if let Some(inode) = self.overlay_inode(serial, root.as_ref(), deadline)? {
            if inode.symlink {
                return Err(WorkspaceError::Io);
            }
            if inode.fresh || inode.captured {
                return Ok((inode.attributes(selected), [0; 32], [0; 32], baseline));
            }
        }
        let response = self.inspect_view(
            operation,
            base,
            Inspect::InodeAttributes { serial },
            deadline,
        )?;
        let (attr, content, metadata) =
            attributes(response, false, self.inner.root.uid, self.inner.root.gid)?;
        // A stale baseline refreshes the inode by identity. Its link count may
        // have changed in the Commit that moved the baseline.
        if attr.serial != serial || attr.kind != selected.kind {
            return Err(WorkspaceError::InvalidInput);
        }
        Ok((attr, content, metadata, baseline))
    }
}
