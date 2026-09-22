//! Original facts for a live file; unresolved local identities never query absent B paths.
use super::{
    namespace::{attributes, check_access, child_path},
    namespace_view::View,
};
use crate::{runtime::state::OperationGuard, *};
use layerfs_bridge::contract::{Inspect, Root};
use std::time::Instant;

pub(super) type Original = (NodeAttributes, Root, Root, u64);

impl Workspace {
    pub(super) fn edit_original(
        &self,
        path: &WorkspacePath,
        deadline: Instant,
        operation: &mut OperationGuard,
    ) -> Result<Original, WorkspaceError> {
        let cached = {
            let state = self.state()?;
            self.available(&state)?;
            if let Some(node) = state.nodes.iter().find(|node| node.path() == path.as_ref()) {
                if node.baseline == state.baseline {
                    return Ok((node.original, node.content, node.metadata, state.baseline));
                }
                Some(node.attr.serial)
            } else {
                None
            }
        };
        if let Some(serial) = cached {
            return self.serial_original(serial, deadline, operation);
        }
        operation.local_io()?;
        let (view, baseline, mut parent) = {
            let state = self.state()?;
            self.available(&state)?;
            (
                View {
                    base: state.base,
                    root: state.overlay.clone(),
                },
                state.baseline,
                state.node(self.inner.root.serial)?.attr,
            )
        };
        let mut bytes = Vec::new();
        let mut result = None;
        for name in path.as_ref().split(|byte| *byte == b'/') {
            if parent.kind != NodeKind::Directory {
                return Err(WorkspaceError::NotDirectory);
            }
            check_access(parent, self.inner.root.uid, 1)?;
            let resolved =
                self.resolve_child(operation, &view, parent.serial, &bytes, name, deadline)?;
            bytes = child_path(&bytes, name)?;
            parent = resolved.attr;
            result = Some((
                resolved.original,
                resolved.content,
                resolved.metadata,
                baseline,
            ));
        }
        result.ok_or(WorkspaceError::InvalidInput)
    }

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
        if attr.serial != serial
            || attr.kind != selected.kind
            || attr.references != selected.references
        {
            return Err(WorkspaceError::InvalidInput);
        }
        Ok((attr, content, metadata, baseline))
    }
}
