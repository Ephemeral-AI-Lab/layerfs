//! Prepare the two attached parents whose deltas one rename will edit.
use super::{
    namespace::{check_access, check_name},
    namespace_view::View,
};
use crate::{overlay::directories::Directory, *};
use std::time::Instant;

pub(super) struct Parent {
    pub serial: u64,
    pub directory: Directory,
    pub dirty: bool,
    pub reanchor: bool,
    pub remove: Option<Vec<u8>>,
    pub bind: Option<(Vec<u8>, u64, NodeKind)>,
    pub mtime: (i64, u32),
}

impl Workspace {
    pub(super) fn rename_parents(
        &self,
        view: &View,
        source: (u64, &[u8]),
        destination: (u64, &[u8]),
        deadline: Instant,
    ) -> Result<Vec<Parent>, WorkspaceError> {
        let (source_parent, source_name) = source;
        let (destination_parent, destination_name) = destination;
        if source_parent == self.inner.root.serial && source_name.is_empty() {
            return Err(WorkspaceError::Unsupported);
        }
        let generation = self.state()?.generation;
        let mut parents = Vec::new();
        parents
            .try_reserve_exact(if source_parent == destination_parent {
                1
            } else {
                2
            })
            .map_err(|_| WorkspaceError::Capacity)?;
        for (serial, name) in [
            (source_parent, source_name),
            (destination_parent, destination_name),
        ] {
            if parents
                .iter()
                .any(|parent: &Parent| parent.serial == serial)
            {
                continue;
            }
            let attr = {
                let state = self.state()?;
                let node = state.node(serial)?;
                if node.attr.kind != NodeKind::Directory {
                    return Err(WorkspaceError::NotDirectory);
                }
                if !node.attached {
                    return Err(WorkspaceError::NotFound);
                }
                check_access(node.attr, self.inner.root.uid, 3)?;
                node.attr
            };
            check_name(name)?;
            let loaded = self.directory_record(view, serial, deadline)?;
            let directory = loaded.unwrap_or_else(|| Directory::initial(attr, view.base));
            let (reanchor, dirty) = {
                let host = self
                    .host
                    .metadata
                    .as_ref()
                    .ok_or(WorkspaceError::Unsupported)?;
                let _view = host.writer()?;
                let mut lease = host.payloads.window(1, 3)?;
                self.directory_delta(
                    view.root.as_ref(),
                    generation,
                    serial,
                    loaded.as_ref(),
                    lease.window.as_mut().ok_or(WorkspaceError::Io)?,
                    deadline,
                )?
            };
            parents.push(Parent {
                serial,
                directory,
                dirty,
                reanchor,
                remove: None,
                bind: None,
                mtime: (attr.mtime_seconds, attr.mtime_nanoseconds),
            });
        }
        Ok(parents)
    }
}
