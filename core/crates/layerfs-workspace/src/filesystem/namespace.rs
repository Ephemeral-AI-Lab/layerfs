use crate::{
    runtime::state::{Node, NODE_LIMIT, PATH_BYTES},
    *,
};
use layerfs_bridge::contract::{Inspect, Operation, Response, Root};
use std::time::Instant;

pub(crate) fn attributes(
    response: Response,
    root: bool,
    uid: u32,
    gid: u32,
) -> Result<(NodeAttributes, Root), WorkspaceError> {
    response.validate_attributes(Some(root))?;
    let Response::Attributes {
        serial,
        kind,
        references,
        content,
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
    if mask & 2 != 0 {
        return Err(WorkspaceError::ReadOnly);
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
        let _operation = self.begin(true, deadline)?;
        let path = {
            let state = self.state()?;
            if scope == ReferenceScope::Projection && !state.mounted {
                return Err(WorkspaceError::Busy);
            }
            let parent = state.node(parent)?;
            if parent.attr.kind != NodeKind::Directory {
                return Err(WorkspaceError::NotDirectory);
            }
            check_access(parent.attr, self.inner.root.uid, 1)?;
            child_path(parent.path(), name)?
        };
        let response = self.call(
            Operation::Inspect {
                root: self.inner.base,
                query: Inspect::Attributes { path: path.clone() },
            },
            0,
            &mut std::io::sink(),
            deadline,
        )?;
        let (attr, content) =
            attributes(response, false, self.inner.root.uid, self.inner.root.gid)?;
        let mut state = self.state()?;
        if let Some(node) = state
            .nodes
            .iter_mut()
            .find(|node| node.attr.serial == attr.serial)
        {
            if node.attr != attr || node.content != content {
                return Err(WorkspaceError::InvalidInput);
            }
            let references = node.references(scope);
            *references = references.checked_add(1).ok_or(WorkspaceError::Capacity)?;
        } else {
            if state.nodes.len() == NODE_LIMIT {
                return Err(WorkspaceError::Capacity);
            }
            let mut node = Node::new(attr, content, &path, parent);
            *node.references(scope) = 1;
            state.nodes.push(node);
        }
        Ok(attr)
    }
    pub fn getattr(&self, serial: u64) -> Result<NodeAttributes, WorkspaceError> {
        let state = self.state()?;
        self.available(&state)?;
        Ok(state.node(serial)?.attr)
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
        check_access(self.getattr(serial)?, uid, mask)
    }
}
