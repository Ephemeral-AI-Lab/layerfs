use crate::{
    runtime::state::{Node, NodeName},
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
pub(crate) fn check_name(name: &[u8]) -> Result<(), WorkspaceError> {
    if name.is_empty()
        || name.len() > 255
        || name == b"."
        || name == b".."
        || name.iter().any(|byte| matches!(byte, 0 | b'/' | b'\\'))
        || std::str::from_utf8(name).is_err()
    {
        return Err(WorkspaceError::InvalidInput);
    }
    Ok(())
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
        let (view, baseline, revision) = {
            let state = self.state()?;
            if scope == ReferenceScope::Projection && !state.mounted {
                return Err(WorkspaceError::Busy);
            }
            let parent = state.node(parent)?;
            if parent.attr.kind != NodeKind::Directory {
                return Err(WorkspaceError::NotDirectory);
            }
            if !parent.attached {
                return Err(WorkspaceError::NotFound);
            }
            check_access(parent.attr, self.inner.root.uid, 1)?;
            (self.selected_view(&state)?, state.baseline, state.revision)
        };
        let resolved = self.resolve_child(&mut operation, &view, parent, name, deadline)?;
        let mut state = self.state()?;
        if state.revision != revision || state.baseline != baseline {
            return Err(WorkspaceError::Busy);
        }
        state.live_chain(parent, None, self.inner.root.serial)?;
        self.cache_lookup(&mut state, resolved, name, parent, scope, baseline)
    }
    pub(super) fn cache_lookup(
        &self,
        state: &mut crate::runtime::state::State,
        resolved: super::namespace_view::Resolved,
        name: &[u8],
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
        let pending_name = if state.is_active
            && canonical
            && !state.inherited_names.contains_key(&(parent, name.to_vec()))
        {
            let charge = self.host.budget.reserve(96 + name.len())?;
            Some((name.to_vec(), charge))
        } else {
            None
        };
        let is_active = state.is_active;
        if let Ok(node) = state.node_mut(attr.serial) {
            if node.attr.kind == NodeKind::Directory
                && (!node.attached || node.parent != parent || node.name.bytes.as_ref() != name)
            {
                return Err(WorkspaceError::InvalidInput);
            }
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
                && !is_active
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
            let name = NodeName::new(name, &self.host.budget)?;
            let mut node = Node::new(original, content, metadata, name, parent);
            node.attr = attr;
            node.baseline = if canonical { baseline } else { 0 };
            // A newly resolved identity starts from the live namespace link
            // count this state already tracks, not from a single name.
            node.names = state.resolved_names(&attr);
            *node.references(scope) = 1;
            state.push_node(node);
        }
        if let Some((name, charge)) = pending_name {
            state
                .inherited_names
                .insert((parent, name), (attr.serial, charge));
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
