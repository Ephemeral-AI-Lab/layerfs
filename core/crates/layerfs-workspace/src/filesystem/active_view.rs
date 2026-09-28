//! Name resolution and bounded listing against one pinned active revision.
use super::{
    namespace::attributes,
    namespace::child_path,
    namespace_view::{Resolved, View},
};
use crate::{
    backing::active::{
        inode_key, namespace_key, ActiveSnapshot, HotInode, NamespaceRecord, ScanPage,
    },
    backing::budget::{Budget, Charge},
    runtime::state::{OperationGuard, State},
    *,
};
use layerfs_bridge::contract::{Inspect, Response};
use std::{collections::BTreeMap, sync::Arc, time::Instant};

/// Original canonical paths for moved inherited directories. Each pinned view
/// retains the map from its own publication revision, including directory handles.
pub(crate) struct ActiveOrigins {
    paths: BTreeMap<u64, Vec<u8>>,
    _charge: Charge,
}

impl ActiveOrigins {
    pub(crate) fn empty(budget: &Arc<Budget>) -> Result<Arc<Self>, WorkspaceError> {
        Ok(Arc::new(Self {
            paths: BTreeMap::new(),
            _charge: budget.reserve(0)?,
        }))
    }

    pub(crate) fn path<'a>(&'a self, serial: u64, current: &'a [u8]) -> &'a [u8] {
        self.paths.get(&serial).map_or(current, Vec::as_slice)
    }

    pub(crate) fn moved(
        self: &Arc<Self>,
        serial: u64,
        current: &[u8],
        budget: &Arc<Budget>,
    ) -> Result<Arc<Self>, WorkspaceError> {
        if self.paths.contains_key(&serial) {
            return Ok(self.clone());
        }
        let bytes = self
            .paths
            .values()
            .map(|path| path.len() + 96)
            .sum::<usize>()
            + current.len()
            + 96;
        let charge = budget.reserve(bytes)?;
        let mut paths = self.paths.clone();
        paths.insert(serial, current.to_vec());
        Ok(Arc::new(Self {
            paths,
            _charge: charge,
        }))
    }
}

struct ActiveNames<'a> {
    snapshot: &'a ActiveSnapshot,
    parent: u64,
    lower: Vec<u8>,
    upper: Vec<u8>,
    page: Option<ScanPage>,
    at: usize,
}

impl<'a> ActiveNames<'a> {
    fn new(
        snapshot: &'a ActiveSnapshot,
        parent: u64,
        after: &[u8],
    ) -> Result<Self, WorkspaceError> {
        if parent == 0 {
            return Err(WorkspaceError::InvalidInput);
        }
        let mut lower = [vec![b'N'], parent.to_be_bytes().to_vec()].concat();
        if !after.is_empty() {
            lower = namespace_key(parent, after)?;
            lower.push(0);
        }
        let upper = if parent == u64::MAX {
            vec![b'O']
        } else {
            [vec![b'N'], (parent + 1).to_be_bytes().to_vec()].concat()
        };
        Ok(Self {
            snapshot,
            parent,
            lower,
            upper,
            page: None,
            at: 0,
        })
    }

    fn next(&mut self) -> Result<Option<(Vec<u8>, NamespaceRecord)>, WorkspaceError> {
        loop {
            if let Some((key, value)) = self
                .page
                .as_ref()
                .and_then(|page| page.entries().get(self.at))
            {
                self.at += 1;
                if key.len() < 10
                    || key[0] != b'N'
                    || key[1..9] != self.parent.to_be_bytes()
                    || namespace_key(self.parent, &key[9..])? != *key
                {
                    return Err(WorkspaceError::Io);
                }
                return Ok(Some((key[9..].to_vec(), NamespaceRecord::parse(value)?)));
            }
            let page = self.snapshot.scan(&self.lower, &self.upper, 128)?;
            let Some((last, _)) = page.entries().last() else {
                return Ok(None);
            };
            self.lower = last.clone();
            self.lower.push(0);
            self.page = Some(page);
            self.at = 0;
        }
    }
}

impl Workspace {
    pub(crate) fn selected_view(&self, state: &State) -> Result<View, WorkspaceError> {
        if self.inner.active.is_some() {
            if state.overlay.is_some() {
                return Err(WorkspaceError::Io);
            }
            let active = self
                .inner
                .active
                .as_ref()
                .ok_or(WorkspaceError::Unsupported)?;
            Ok(View {
                base: state.base,
                root: None,
                active: Some(Arc::new(active.pin_view()?)),
                origins: Some(state.active_origins.clone()),
                directory_path: None,
            })
        } else {
            Ok(View {
                base: state.base,
                root: state.overlay.clone(),
                active: None,
                origins: None,
                directory_path: None,
            })
        }
    }

    pub(crate) fn resolve_child_active(
        &self,
        operation: &mut OperationGuard,
        view: &View,
        parent: u64,
        path: &[u8],
        name: &[u8],
        deadline: Instant,
    ) -> Result<Resolved, WorkspaceError> {
        if view.root.is_some() {
            return Err(WorkspaceError::Io);
        }
        let active = view.active.as_ref().ok_or(WorkspaceError::Io)?;
        let origin = view.origins.as_ref().ok_or(WorkspaceError::Io)?;
        let canonical_path = origin.path(parent, path);
        let child = child_path(canonical_path, name)?;
        if let Some(value) = active.get(&namespace_key(parent, name)?)? {
            let binding = NamespaceRecord::parse(&value)?;
            if binding.tombstone {
                return Err(WorkspaceError::NotFound);
            }
            let inode = HotInode::parse(
                &active
                    .get(&inode_key(binding.serial))?
                    .ok_or(WorkspaceError::Io)?,
            )?;
            let template = NodeAttributes {
                serial: binding.serial,
                kind: binding.kind,
                size: 0,
                references: 1,
                mode: inode.mode,
                mtime_seconds: inode.seconds,
                mtime_nanoseconds: inode.nanos,
                uid: self.inner.root.uid,
                gid: self.inner.root.gid,
            };
            let attr = inode.attributes(template)?;
            return Ok(Resolved {
                original: if inode.fresh { template } else { attr },
                attr,
                content: if inode.fresh { [0; 32] } else { inode.base },
                metadata: inode.metadata,
                canonical: false,
                base: [0; 32],
            });
        }
        if active
            .get(&inode_key(parent))?
            .map(|value| HotInode::parse(&value))
            .transpose()?
            .is_some_and(|inode| inode.fresh)
        {
            return Err(WorkspaceError::NotFound);
        }
        let response = self.inspect_view(
            operation,
            view.base,
            Inspect::Attributes { path: child },
            deadline,
        )?;
        let (original, content, metadata) =
            attributes(response, false, self.inner.root.uid, self.inner.root.gid)?;
        let attr = match active.get(&inode_key(original.serial))? {
            Some(value) => HotInode::parse(&value)?.attributes(original)?,
            None => original,
        };
        Ok(Resolved {
            original,
            attr,
            content,
            metadata,
            canonical: true,
            base: view.base,
        })
    }

    /// Merge active name bindings/tombstones with the canonical byte-ordered
    /// listing, advancing both cursors only as far as this page needs.
    pub(crate) fn list_view_active(
        &self,
        operation: &mut OperationGuard,
        view: &View,
        directory: (u64, &[u8]),
        after: &[u8],
        limit: usize,
        deadline: Instant,
    ) -> Result<Vec<(Vec<u8>, u64)>, WorkspaceError> {
        if view.root.is_some() || limit == 0 || limit > 128 {
            return Err(WorkspaceError::InvalidInput);
        }
        let active = view.active.as_ref().ok_or(WorkspaceError::Io)?;
        let (serial, path) = directory;
        let path = view
            .origins
            .as_ref()
            .ok_or(WorkspaceError::Io)?
            .path(serial, path);
        let mut names = crate::backing::metadata_index::vector(limit)?;
        let mut active_names = ActiveNames::new(active, serial, after)?;
        let mut local = active_names.next()?;
        let fresh = active
            .get(&inode_key(serial))?
            .map(|value| HotInode::parse(&value))
            .transpose()?
            .is_some_and(|inode| inode.fresh);
        let base = (!fresh).then_some(view.base);
        let mut continuation = base.map(|_| after.to_vec());
        let mut canonical = Vec::new();
        let mut canonical_at = 0;
        let mut remote = None;
        while names.len() < limit {
            if remote.is_none() && canonical_at == canonical.len() {
                if let (Some(base), Some(after_name)) = (base, continuation.take()) {
                    let response = self.inspect_view(
                        operation,
                        base,
                        Inspect::List {
                            path: path.to_vec(),
                            after: after_name.clone(),
                            entries: 128,
                            bytes: 16384,
                        },
                        deadline,
                    )?;
                    let Response::List {
                        entries,
                        continuation: next,
                    } = response
                    else {
                        return Err(WorkspaceError::InvalidInput);
                    };
                    if entries.len() > 128
                        || entries
                            .iter()
                            .map(|(name, _)| name.len() + 10)
                            .sum::<usize>()
                            > 16384
                        || next
                            .as_ref()
                            .is_some_and(|name| entries.last().is_none_or(|last| &last.0 != name))
                        || (entries.is_empty() && next.is_some())
                    {
                        return Err(WorkspaceError::InvalidInput);
                    }
                    let mut prior = after_name.as_slice();
                    for (name, _) in &entries {
                        if name.as_slice() <= prior {
                            return Err(WorkspaceError::InvalidInput);
                        }
                        child_path(path, name)?;
                        prior = name;
                    }
                    canonical = entries;
                    canonical_at = 0;
                    continuation = next;
                }
            }
            if remote.is_none() {
                remote = canonical.get(canonical_at).cloned();
                if remote.is_some() {
                    canonical_at += 1;
                }
            }
            match (&local, &remote) {
                (None, None) => break,
                (Some((name, _)), Some((other, _))) if name <= other => {
                    let (name, binding) = local.take().ok_or(WorkspaceError::Io)?;
                    if &name == other {
                        remote = None;
                    }
                    if !binding.tombstone {
                        names.push((name, binding.serial));
                    }
                    local = active_names.next()?;
                }
                (Some(_), None) => {
                    let (name, binding) = local.take().ok_or(WorkspaceError::Io)?;
                    if !binding.tombstone {
                        names.push((name, binding.serial));
                    }
                    local = active_names.next()?;
                }
                _ => names.push(remote.take().ok_or(WorkspaceError::Io)?),
            }
        }
        Ok(names)
    }
}
