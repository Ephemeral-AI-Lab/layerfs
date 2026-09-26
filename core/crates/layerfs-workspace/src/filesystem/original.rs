//! Original facts for a live file; unresolved local identities never query absent B paths.
use super::{namespace::attributes, namespace_view::View};
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
        let _path = self
            .host
            .budget
            .reserve(crate::runtime::state::PATH_BYTES)?;
        let (view, baseline, path, path_len, selected) = {
            let state = self.state()?;
            self.available(&state)?;
            let node = state.node(serial)?;
            if node.attr.kind == NodeKind::Directory {
                return Err(WorkspaceError::IsDirectory);
            }
            if node.attr.kind != NodeKind::File {
                return Err(WorkspaceError::WrongKind);
            }
            if node.baseline == state.baseline {
                return Ok((node.original, node.content, node.metadata, state.baseline));
            }
            (
                View {
                    base: state.base,
                    root: state.overlay.clone(),
                },
                state.baseline,
                node.path,
                node.path_len,
                node.attr,
            )
        };
        if let Some(inode) = self.overlay_inode(serial, view.root.as_ref(), deadline)? {
            if inode.symlink {
                return Err(WorkspaceError::Io);
            }
            if inode.fresh || inode.captured {
                return Ok((inode.attributes(selected), [0; 32], [0; 32], baseline));
            }
        }
        let mut bytes = crate::backing::metadata_index::vector(path_len)?;
        bytes.extend_from_slice(&path[..path_len]);
        let response = self.inspect_view(
            operation,
            view.base,
            Inspect::Attributes { path: bytes },
            deadline,
        )?;
        let (attr, content, metadata) =
            attributes(response, false, self.inner.root.uid, self.inner.root.gid)?;
        // This query runs only when the node's baseline is stale, so the cached
        // path must still name the same identity. Its link count is not part of
        // that stability: the Commit that moved the baseline may have republished
        // the identity with a changed count, and this resolution is the refresh.
        if attr.serial != serial || attr.kind != selected.kind {
            return Err(WorkspaceError::InvalidInput);
        }
        Ok((attr, content, metadata, baseline))
    }
}
