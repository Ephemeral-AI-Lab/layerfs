use crate::{
    runtime::state::{Node, NODE_LIMIT, PATH_BYTES},
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
    if name.is_empty()
        || name.len() > 255
        || name == b"."
        || name == b".."
        || name.iter().any(|byte| matches!(byte, 0 | b'/' | b'\\'))
        || std::str::from_utf8(name).is_err()
    {
        return Err(WorkspaceError::InvalidInput);
    }
    let total = parent.len() + usize::from(!parent.is_empty()) + name.len();
    if total > PATH_BYTES
        || parent.iter().filter(|byte| **byte == b'/').count() + usize::from(!parent.is_empty())
            >= 256
    {
        return Err(WorkspaceError::Capacity);
    }
    let mut result = Vec::new();
    result
        .try_reserve_exact(total)
        .map_err(|_| WorkspaceError::Capacity)?;
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
    /// The one directory-delta decision every namespace mutation shares: whether
    /// the maintained delta must be re-anchored on the current root, and whether
    /// this generation's ledger already carries the exact dirty key.
    ///
    /// The record's own generation cannot answer the second question, because it
    /// only advances at capture. The origin answers the first: a delta that is
    /// already captured or empty keeps the entries it inherited.
    pub(crate) fn directory_delta(
        &self,
        root: Option<&std::sync::Arc<crate::backing::metadata::RootOwner>>,
        generation: u64,
        serial: u64,
        record: Option<&crate::overlay::directories::Directory>,
        window: &mut crate::backing::segments::Window,
        deadline: Instant,
    ) -> Result<(bool, bool), WorkspaceError> {
        let maintained = record.is_some_and(|directory| {
            matches!(
                directory.origin,
                crate::overlay::directories::Origin::Captured(_)
                    | crate::overlay::directories::Origin::Empty
            )
        });
        let dirty = self.dirty_in_generation(root, generation, serial, window, deadline)?;
        Ok((!maintained, dirty))
    }
    /// True when this generation's ledger already carries the exact dirty key
    /// for `serial`. A directory with a maintained delta is not a new dirty
    /// inode, and one without it is, whatever record it still holds.
    pub(crate) fn dirty_in_generation(
        &self,
        root: Option<&std::sync::Arc<crate::backing::metadata::RootOwner>>,
        generation: u64,
        serial: u64,
        window: &mut crate::backing::segments::Window,
        deadline: Instant,
    ) -> Result<bool, WorkspaceError> {
        let Some(root) = root else { return Ok(false) };
        Ok(root
            .arena
            .find(
                root.root()?,
                &crate::backing::metadata_pages::dirty_key(generation, serial),
                window,
                deadline,
            )?
            .is_some_and(|cell| cell.value() == [1]))
    }
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
        let (path, base, baseline, revision, root) = {
            let state = self.state()?;
            if scope == ReferenceScope::Projection && !state.mounted {
                return Err(WorkspaceError::Busy);
            }
            let parent = state.node(parent)?;
            if parent.attr.kind != NodeKind::Directory {
                return Err(WorkspaceError::NotDirectory);
            }
            check_access(parent.attr, self.inner.root.uid, 1)?;
            (
                parent.path().to_vec(),
                state.base,
                state.baseline,
                state.revision,
                state.overlay.clone(),
            )
        };
        let view = super::namespace_view::View { base, root };
        let resolved = self.resolve_child(&mut operation, &view, parent, &path, name, deadline)?;
        let path = child_path(&path, name)?;
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
        if let Some(node) = state
            .nodes
            .iter_mut()
            .find(|node| node.attr.serial == attr.serial)
        {
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
            if node.baseline == baseline && node.attr.references != original.references {
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
            if state.nodes.len() == NODE_LIMIT {
                return Err(WorkspaceError::Capacity);
            }
            let mut node = Node::new(original, content, metadata, path, parent);
            node.attr = attr;
            node.baseline = if canonical { baseline } else { 0 };
            // A newly resolved identity starts from the live namespace link
            // count this state already tracks, not from a single name.
            node.names = state.resolved_names(&attr);
            *node.references(scope) = 1;
            state.nodes.push(node);
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
