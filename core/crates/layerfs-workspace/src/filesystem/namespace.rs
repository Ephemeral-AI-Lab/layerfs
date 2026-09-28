use crate::{
    runtime::state::{Node, PATH_BYTES},
    *,
};
use layerfs_bridge::contract::{Response, Root};
use std::time::Instant;

pub(crate) fn attributes(
    response: Response,
    root: bool,
    uid: u32,
    gid: u32,
) -> Result<(NodeAttributes, Root, Root), WorkspaceError> {
    response.validate_attributes(Some(root))?;
    let Response::Attributes {
        serial,
        kind,
        references,
        content,
        metadata,
        mode,
        mtime,
        nanoseconds,
        size,
        ..
    } = response
    else {
        return Err(WorkspaceError::InvalidInput);
    };
    let kind = match kind {
        1 => NodeKind::File,
        2 => NodeKind::Directory,
        3 => NodeKind::Symlink,
        _ => return Err(WorkspaceError::InvalidInput),
    };
    Ok((
        NodeAttributes {
            serial,
            kind,
            size,
            references,
            mode,
            mtime_seconds: mtime,
            mtime_nanoseconds: nanoseconds,
            uid,
            gid,
        },
        content,
        metadata,
    ))
}
pub(crate) fn child_path(parent: &[u8], name: &[u8]) -> Result<Vec<u8>, WorkspaceError> {
    child_path_with_limit(parent, name, PATH_BYTES)
}
/// Active backing resolves inherited names by inode identity, not by this
/// materialized locator. Bound the latter by its component count and charge.
pub(crate) fn child_path_active(parent: &[u8], name: &[u8]) -> Result<Vec<u8>, WorkspaceError> {
    let bytes = child_path_with_limit(parent, name, crate::runtime::state::ACTIVE_PATH_BYTES)?;
    Ok(bytes)
}
fn child_path_with_limit(
    parent: &[u8],
    name: &[u8],
    limit: usize,
) -> Result<Vec<u8>, WorkspaceError> {
    if name.is_empty()
        || name.len() > 255
        || name == b"."
        || name == b".."
        || name.iter().any(|byte| matches!(byte, 0 | b'/' | b'\\'))
        || std::str::from_utf8(name).is_err()
    {
        return Err(WorkspaceError::InvalidInput);
    }
    let total = parent
        .len()
        .checked_add(usize::from(!parent.is_empty()))
        .and_then(|n| n.checked_add(name.len()))
        .ok_or(WorkspaceError::Capacity)?;
    if total > limit
        || parent.iter().filter(|byte| **byte == b'/').count() + usize::from(!parent.is_empty())
            >= 256
    {
        return Err(WorkspaceError::Capacity);
    }
    let mut result = crate::backing::metadata_index::vector(total)?;
    result.extend_from_slice(parent);
    if !parent.is_empty() {
        result.push(b'/');
    }
    result.extend_from_slice(name);
    Ok(result)
}
pub(crate) fn check_access(attr: NodeAttributes, uid: u32, mask: u8) -> Result<(), WorkspaceError> {
    if mask & !7 != 0 {
        return Err(WorkspaceError::InvalidInput);
    }
    if uid != attr.uid {
        return Err(WorkspaceError::Denied);
    }

    if uid == 0 {
        if attr.kind == NodeKind::File && mask & 1 != 0 && attr.mode & 0o111 == 0 {
            return Err(WorkspaceError::Denied);
        }
    } else if u32::from(mask) & !(attr.mode >> 6) != 0 {
        return Err(WorkspaceError::Denied);
    }
    Ok(())
}
impl Workspace {
    pub fn lookup(
        &self,
        parent: u64,
        name: &[u8],
        scope: ReferenceScope,
        deadline: Instant,
    ) -> Result<NodeAttributes, WorkspaceError> {
        let deadline = Self::callback_deadline(deadline);
        let mut operation = self.begin(false, deadline)?;
        operation.local_io()?;
        let (path, baseline, revision, view) = {
            let state = self.state()?;
            if scope == ReferenceScope::Projection && !state.mounted {
                return Err(WorkspaceError::Busy);
            }
            let parent = state.node(parent)?;
            if parent.attr.kind != NodeKind::Directory {
                return Err(WorkspaceError::NotDirectory);
            }
            if self.inner.active.is_some() && !parent.attached {
                return Err(WorkspaceError::NotFound);
            }
            check_access(parent.attr, self.inner.root.uid, 1)?;
            (
                parent.path().to_vec(),
                state.baseline,
                state.revision,
                self.selected_view(&state)?,
            )
        };
        let resolved = self.resolve_child(&mut operation, &view, parent, &path, name, deadline)?;
        let path = if self.inner.active.is_some() {
            child_path_active(&path, name)?
        } else {
            child_path(&path, name)?
        };
        let mut state = self.state()?;
        if state.revision != revision || state.baseline != baseline {
            return Err(WorkspaceError::Busy);
        }
        self.cache_lookup(&mut state, resolved, &path, parent, scope, baseline)
    }
    pub(super) fn cache_lookup(
        &self,
        state: &mut crate::runtime::state::State,
        resolved: super::namespace_view::Resolved,
        path: &[u8],
        parent: u64,
        scope: ReferenceScope,
        baseline: u64,
    ) -> Result<NodeAttributes, WorkspaceError> {
        let super::namespace_view::Resolved {
            original,
            attr,
            content,
            metadata,
            canonical,
            ..
        } = resolved;
        if let Ok(node) = state.node_mut(attr.serial) {
            if canonical
                && node.baseline == baseline
                && (node.original != original
                    || node.content != content
                    || node.metadata != metadata)
            {
                return Err(WorkspaceError::InvalidInput);
            }
            if node.attr.kind != original.kind {
                return Err(WorkspaceError::InvalidInput);
            }
            // Within one baseline the resident node and this resolution must
            // agree on the namespace link count. A Commit that republished the
            // identity canonically can change it (a link or a removal), and the
            // node then refreshes exactly as its original and roots do below.
            if canonical
                && self.inner.active.is_none()
                && node.baseline == baseline
                && node.attr.references != original.references
            {
                return Err(WorkspaceError::InvalidInput);
            }
            node.original = original;
            node.content = content;
            node.metadata = metadata;
            node.baseline = if canonical { baseline } else { 0 };
            node.attr = attr;
            let references = node.references(scope);
            *references = references.checked_add(1).ok_or(WorkspaceError::Capacity)?;
        } else {
            state.reserve_nodes()?;
            let mut node = if self.inner.active.is_some() {
                let prepared = crate::runtime::state::NodePath::new(path, &self.host.budget)?;
                Node::with_path(original, content, metadata, parent, prepared)
            } else {
                Node::new(original, content, metadata, path, parent)
            };
            node.attr = attr;
            node.baseline = if canonical { baseline } else { 0 };
            // A newly resolved identity starts from the live namespace link
            // count this state already tracks, not from a single name.
            node.names = state.resolved_names(&attr);
            *node.references(scope) = 1;
            state.push_node(node);
        }
        Ok(state.presented(attr))
    }
    pub fn getattr(&self, serial: u64) -> Result<NodeAttributes, WorkspaceError> {
        let state = self.state()?;
        self.available(&state)?;
        Ok(state.presented(state.node(serial)?.attr))
    }
    pub fn forget(&self, serial: u64, count: u64, scope: ReferenceScope) {
        if let Ok(mut state) = self.state() {
            if let Ok(node) = state.node_mut(serial) {
                let references = node.references(scope);
                *references = references.saturating_sub(count);
            }
            state.collect(self.inner.root.serial);
        }
    }
    /// Owner-only presentation plus portable mode checks; Store grants remain independent.
    pub fn access(&self, serial: u64, uid: u32, _gid: u32, mask: u8) -> Result<(), WorkspaceError> {
        let attr = self.getattr(serial)?;
        if mask & !7 != 0 {
            return Err(WorkspaceError::InvalidInput);
        }
        if uid != attr.uid {
            return Err(WorkspaceError::Denied);
        }
        if mask & 2 != 0 && self.inner.access == WorkspaceAccess::ReadOnly {
            return Err(WorkspaceError::ReadOnly);
        }
        check_access(attr, uid, mask)
    }
}
