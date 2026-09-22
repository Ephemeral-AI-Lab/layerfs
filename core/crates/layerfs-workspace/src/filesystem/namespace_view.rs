//! View-bound namespace resolution; local index guards end before service reads.
use super::namespace::{attributes, child_path};
use crate::{
    backing::{metadata::RootOwner, metadata_pages},
    overlay::directories::{self, Directory, Origin},
    *,
};
use layerfs_bridge::contract::{Inspect, Operation, Response, Root};
use std::{sync::Arc, time::Instant};

#[derive(Clone)]
pub(crate) struct View {
    pub base: Root,
    pub root: Option<Arc<RootOwner>>,
}
pub(crate) struct Resolved {
    pub original: NodeAttributes,
    pub attr: NodeAttributes,
    pub content: Root,
    pub metadata: Root,
    pub canonical: bool,
    /// The root a path query actually answered from, or zero when the overlay
    /// resolved the name locally. A caller that must re-ask the service about
    /// the same identity (a symlink target, a record for a name this operation
    /// moves) uses this root, never a cached path.
    pub base: Root,
}
impl Workspace {
    pub(crate) fn inspect_view(
        &self,
        operation: &mut crate::runtime::state::OperationGuard,
        base: Root,
        query: Inspect,
        deadline: Instant,
    ) -> Result<Response, WorkspaceError> {
        operation.metadata_call()?;
        let result = self.call(
            Operation::Inspect { root: base, query },
            0,
            &mut std::io::sink(),
            deadline,
        );
        operation.release_metadata_call();
        result
    }
    pub(crate) fn directory_record(
        &self,
        view: &View,
        serial: u64,
        deadline: Instant,
    ) -> Result<Option<Directory>, WorkspaceError> {
        let Some(owner) = &view.root else {
            return Ok(None);
        };
        let host = self
            .host
            .metadata
            .as_ref()
            .ok_or(WorkspaceError::Unsupported)?;
        let _view = host.writer()?;
        let mut lease = host.payloads.window(1, 3)?;
        directories::load(
            owner,
            serial,
            lease.window.as_mut().ok_or(WorkspaceError::Io)?,
            deadline,
        )
    }
    pub(crate) fn resolve_child(
        &self,
        operation: &mut crate::runtime::state::OperationGuard,
        view: &View,
        parent: u64,
        path: &[u8],
        name: &[u8],
        deadline: Instant,
    ) -> Result<Resolved, WorkspaceError> {
        let path = child_path(path, name)?;
        let mut base = Some(view.base);
        let mut binding = None;
        let mut local = None;
        let mut file = None;
        if let Some(owner) = &view.root {
            let host = self
                .host
                .metadata
                .as_ref()
                .ok_or(WorkspaceError::Unsupported)?;
            let _view = host.writer()?;
            let mut lease = host.payloads.window(1, 3)?;
            let window = lease.window.as_mut().ok_or(WorkspaceError::Io)?;
            if let Some(directory) = directories::load(owner, parent, window, deadline)? {
                let key = metadata_pages::entry_key(name)?;
                binding = owner
                    .arena
                    .find(directory.entries, &key, window, deadline)?
                    .map(|cell| directories::entry_info(cell.value()))
                    .transpose()?;
                let origin = match directory.origin {
                    Origin::Captured(reference) => {
                        let prior = directories::captured(owner, reference, window, deadline)?;
                        if binding.is_none() {
                            binding = owner
                                .arena
                                .find(prior.entries, &key, window, deadline)?
                                .map(|cell| directories::entry_info(cell.value()))
                                .transpose()?;
                        }
                        prior.origin
                    }
                    origin => origin,
                };
                // A loaded delta's tombstone overrides its inherited origin. An
                // addition it carries is already the exact binding.
                if binding.is_none()
                    && directories::removed(owner, directory, name, window, deadline)?
                {
                    return Err(WorkspaceError::NotFound);
                }
                base = match origin {
                    Origin::Empty => None,
                    Origin::Canonical(root) => Some(root),
                    Origin::Captured(_) => return Err(WorkspaceError::Io),
                };
            }
            if let Some((serial, kind)) = binding {
                if kind == NodeKind::Directory {
                    local = Some(
                        directories::load(owner, serial, window, deadline)?
                            .ok_or(WorkspaceError::Io)?,
                    );
                } else {
                    let cell = owner
                        .arena
                        .find(
                            owner.root()?,
                            &metadata_pages::inode_key(serial),
                            window,
                            deadline,
                        )?
                        .ok_or(WorkspaceError::Io)?;
                    let inode = crate::overlay::pieces::Inode::parse(cell.value())?;
                    if inode.kind() != kind {
                        return Err(WorkspaceError::Io);
                    }
                    file = Some(inode);
                }
            }
        }
        if let (Some((serial, _)), Some(inode)) = (binding, file) {
            if inode.fresh || inode.captured {
                let original = inode.attributes(NodeAttributes {
                    serial,
                    kind: inode.kind(),
                    size: 0,
                    references: 1,
                    mode: 0,
                    mtime_seconds: 0,
                    mtime_nanoseconds: 0,
                    uid: self.inner.root.uid,
                    gid: self.inner.root.gid,
                });
                return Ok(Resolved {
                    original,
                    attr: original,
                    content: [0; 32],
                    metadata: [0; 32],
                    canonical: false,
                    base: [0; 32],
                });
            }
            // A name this generation added for an identity the attached base
            // still owns has no path the service can resolve: the identity's own
            // locator answers, and the record supplies its effective attributes.
            let (original, content, metadata, _) =
                self.serial_original(serial, deadline, operation)?;
            if original.kind != inode.kind() {
                return Err(WorkspaceError::Io);
            }
            let attr = inode.attributes(original);
            return Ok(Resolved {
                original,
                attr,
                content,
                metadata,
                canonical: true,
                base: view.base,
            });
        }
        if let (Some((serial, _)), Some(directory)) = (binding, local) {
            match directory.origin {
                Origin::Empty | Origin::Captured(_) => {
                    let original = directory.attributes(NodeAttributes {
                        serial,
                        kind: NodeKind::Directory,
                        size: 0,
                        references: 1,
                        mode: 0,
                        mtime_seconds: 0,
                        mtime_nanoseconds: 0,
                        uid: self.inner.root.uid,
                        gid: self.inner.root.gid,
                    });
                    return Ok(Resolved {
                        original,
                        attr: original,
                        content: [0; 32],
                        metadata: [0; 32],
                        canonical: false,
                        base: [0; 32],
                    });
                }
                Origin::Canonical(root) => base = Some(root),
            }
        }
        let base = base.ok_or(WorkspaceError::NotFound)?;
        let response =
            self.inspect_view(operation, base, Inspect::Attributes { path }, deadline)?;
        let (original, content, metadata) =
            attributes(response, false, self.inner.root.uid, self.inner.root.gid)?;
        if binding.is_some_and(|(serial, kind)| serial != original.serial || kind != original.kind)
        {
            return Err(WorkspaceError::InvalidInput);
        }
        let attr = self.overlay_attributes(original, view.root.as_ref(), deadline)?;
        Ok(Resolved {
            original,
            attr,
            content,
            metadata,
            canonical: base == view.base,
            base,
        })
    }
    pub(crate) fn list_view(
        &self,
        operation: &mut crate::runtime::state::OperationGuard,
        view: &View,
        directory: (u64, &[u8]),
        after: &[u8],
        limit: usize,
        deadline: Instant,
    ) -> Result<Vec<(Vec<u8>, u64)>, WorkspaceError> {
        let (serial, path) = directory;
        let mut names = crate::backing::metadata_index::vector(limit)?;
        let mut base = Some(view.base);
        let mut delta = None;
        if let Some(owner) = &view.root {
            let host = self
                .host
                .metadata
                .as_ref()
                .ok_or(WorkspaceError::Unsupported)?;
            let _view = host.writer()?;
            let mut lease = host.payloads.window(1, 3)?;
            let window = lease.window.as_mut().ok_or(WorkspaceError::Io)?;
            if let Some(directory) = directories::load(owner, serial, window, deadline)? {
                delta = Some(directory);
                let mut roots = [
                    directory.entries,
                    crate::backing::metadata_pages::PageRef::NULL,
                ];
                let origin = match directory.origin {
                    Origin::Captured(reference) => {
                        let prior = directories::captured(owner, reference, window, deadline)?;
                        roots[1] = prior.entries;
                        prior.origin
                    }
                    origin => origin,
                };
                base = match origin {
                    Origin::Empty => None,
                    Origin::Canonical(root) => Some(root),
                    Origin::Captured(_) => return Err(WorkspaceError::Io),
                };
                let mut lower = metadata_pages::entry_key(after)?;
                let mut exclusive = !after.is_empty();
                while names.len() < limit {
                    let a = owner
                        .arena
                        .next(roots[0], &lower, exclusive, window, deadline)?;
                    let b = owner
                        .arena
                        .next(roots[1], &lower, exclusive, window, deadline)?;
                    let next = match (a, b) {
                        (Some(a), Some(b)) => Some(if a.key() <= b.key() { a } else { b }),
                        (a, b) => a.or(b),
                    };
                    let Some(cell) = next else { break };
                    lower = cell.key().to_vec();
                    exclusive = true;
                    let name = &cell.key()[1..];
                    child_path(path, name)?;
                    let removal = directories::tombstone_key(name)?;
                    if owner
                        .arena
                        .find(directory.tombstones, &removal, window, deadline)?
                        .is_some()
                    {
                        continue;
                    }
                    names.push((name.to_vec(), directories::entry_serial(cell.value())?));
                }
            }
        }
        let Some(base) = base else {
            return Ok(names);
        };
        // The origin still lists every name it holds, including the ones this
        // delta removed. A tombstone means absent from the effective namespace,
        // so the merge drops those names and asks the origin for the next page
        // instead of returning a short listing: a mounted caller reads a page
        // that is not full as the end of the directory.
        let mut cursor = after.to_vec();
        loop {
            let response = self.inspect_view(
                operation,
                base,
                Inspect::List {
                    path: path.to_vec(),
                    after: cursor.clone(),
                    entries: limit as u16,
                    bytes: 16384,
                },
                deadline,
            )?;
            let Response::List {
                entries,
                continuation,
            } = response
            else {
                return Err(WorkspaceError::InvalidInput);
            };
            if entries.len() > limit
                || entries
                    .iter()
                    .map(|(name, _)| name.len() + 10)
                    .sum::<usize>()
                    > 16384
                || continuation
                    .as_ref()
                    .is_some_and(|next| entries.last().is_none_or(|(last, _)| next != last))
            {
                return Err(WorkspaceError::InvalidInput);
            }
            let mut previous = cursor.as_slice();
            for (name, _) in &entries {
                if name.as_slice() <= previous {
                    return Err(WorkspaceError::InvalidInput);
                }
                child_path(path, name)?;
                previous = name;
            }
            let full = entries.len() == limit;
            let mut dropped = false;
            let entries = match (delta, view.root.as_ref()) {
                (Some(directory), Some(owner)) => {
                    let host = self
                        .host
                        .metadata
                        .as_ref()
                        .ok_or(WorkspaceError::Unsupported)?;
                    let _view = host.writer()?;
                    let mut lease = host.payloads.window(1, 3)?;
                    let window = lease.window.as_mut().ok_or(WorkspaceError::Io)?;
                    let mut kept = crate::backing::metadata_index::vector(limit)?;
                    for (name, serial) in entries {
                        if directories::removed(owner, directory, &name, window, deadline)? {
                            dropped = true;
                        } else {
                            kept.push((name, serial));
                        }
                    }
                    kept
                }
                _ => entries,
            };
            for entry in entries {
                match names.binary_search_by(|(name, _)| name.cmp(&entry.0)) {
                    Ok(_) => {}
                    Err(at) if at < limit => {
                        if names.len() == limit {
                            names.pop();
                        }
                        names.insert(at, entry);
                    }
                    Err(_) => {}
                }
            }
            let Some(next) = continuation else { break };
            // One more origin page is read only while this one either removed a
            // name or was full: a page that added nothing and removed nothing
            // cannot repeat, because a name this delta already binds is one of
            // the at most 128 names its own entry page holds.
            if names.len() == limit || (!dropped && !full) {
                break;
            }
            cursor = next;
        }
        Ok(names)
    }
}
