//! Validate a directory move's live paths before publishing its single root.
use super::{namespace::child_path, namespace_view::View};
use crate::{
    runtime::state::{OperationGuard, State, PATH_BYTES},
    NodeKind, Workspace, WorkspaceError,
};
use std::time::Instant;

fn within(path: &[u8], old: &[u8]) -> bool {
    path == old
        || path
            .strip_prefix(old)
            .is_some_and(|tail| tail.first() == Some(&b'/'))
}

fn projected(path: &[u8], old: &[u8], new: &[u8]) -> Result<(), WorkspaceError> {
    let suffix = path.strip_prefix(old).ok_or(WorkspaceError::Io)?;
    let length = new
        .len()
        .checked_add(suffix.len())
        .ok_or(WorkspaceError::Capacity)?;
    let components = usize::from(!new.is_empty())
        + new.iter().filter(|byte| **byte == b'/').count()
        + suffix.iter().filter(|byte| **byte == b'/').count();
    if length > PATH_BYTES || components > 256 {
        return Err(WorkspaceError::Capacity);
    }
    Ok(())
}

/// Recheck under the final state lock: a concurrent lookup can add a pin without
/// changing the namespace revision between preparation and publication.
pub(super) fn check_cached(state: &State, old: &[u8], new: &[u8]) -> Result<(), WorkspaceError> {
    for node in &state.nodes {
        if within(node.path(), old) {
            projected(node.path(), old, new)?;
        }
    }
    Ok(())
}

impl Workspace {
    pub(super) fn preflight_rename_paths(
        &self,
        operation: &mut OperationGuard,
        view: &View,
        serial: u64,
        old: &[u8],
        new: &[u8],
        deadline: Instant,
    ) -> Result<(), WorkspaceError> {
        projected(old, old, new)?;
        let state = self.state()?;
        check_cached(&state, old, new)?;
        drop(state);
        let old_depth = old.iter().filter(|byte| **byte == b'/').count();
        let new_depth = new.iter().filter(|byte| **byte == b'/').count();
        if new.len() > old.len() || new_depth > old_depth {
            // ponytail: growth walks the inherited subtree to prove path bounds;
            // a canonical max-relative-path summary can replace this if it is hot.
            self.scan_rename_descendants(operation, view, serial, old, (old, new), deadline)?;
        }
        Ok(())
    }

    fn scan_rename_descendants(
        &self,
        operation: &mut OperationGuard,
        view: &View,
        serial: u64,
        path: &[u8],
        paths: (&[u8], &[u8]),
        deadline: Instant,
    ) -> Result<(), WorkspaceError> {
        // One frame per path component, never one resident record per descendant.
        let _charge = self.host.budget.reserve(path.len() + 512)?;
        let mut after = Vec::new();
        loop {
            let mut page = self.list_view(operation, view, (serial, path), &after, 1, deadline)?;
            let Some((name, expected)) = page.pop() else {
                break;
            };
            let child = child_path(path, &name)?;
            projected(&child, paths.0, paths.1)?;
            let resolved = self.resolve_child(operation, view, serial, path, &name, deadline)?;
            if resolved.attr.serial != expected {
                return Err(WorkspaceError::Io);
            }
            if resolved.attr.kind == NodeKind::Directory {
                self.scan_rename_descendants(operation, view, expected, &child, paths, deadline)?;
            }
            after = name;
        }
        Ok(())
    }
}
