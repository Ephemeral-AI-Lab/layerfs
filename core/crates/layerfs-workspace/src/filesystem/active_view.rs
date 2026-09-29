//! Name resolution and bounded listing against one pinned active revision.
use super::{
    namespace::attributes,
    namespace::check_name,
    namespace_view::{Resolved, View},
};
use crate::{
    backing::active::{
        inode_key, namespace_key, ActiveSnapshot, HotInode, NamespaceRecord, ScanPage,
    },
    runtime::state::{OperationGuard, State},
    *,
};
use layerfs_bridge::contract::{Inspect, Response};
use std::{sync::Arc, time::Instant};

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
            })
        } else {
            Ok(View {
                base: state.base,
                root: state.overlay.clone(),
                active: None,
            })
        }
    }

    pub(crate) fn resolve_child_active(
        &self,
        operation: &mut OperationGuard,
        view: &View,
        parent: u64,
        name: &[u8],
        deadline: Instant,
    ) -> Result<Resolved, WorkspaceError> {
        if view.root.is_some() {
            return Err(WorkspaceError::Io);
        }
        let active = view.active.as_ref().ok_or(WorkspaceError::Io)?;
        // The base is keyed by immutable inode identity. A moved directory has
        // no canonical path at its new name, but its serial still owns children.
        // The selected index uses parent serial plus exactly one component.
        check_name(name)?;
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
        // A service SaveFile may be in flight while G2 still edits an already
        // resolved name. Canonical facts are immutable within one baseline;
        // never borrow a live entry for a pinned view from an older base.
        let cached = {
            let state = self.state()?;
            if view.base == state.base {
                state
                    .inherited_names
                    .get(&(parent, name.to_vec()))
                    .and_then(|(serial, _)| state.node(*serial).ok())
                    .filter(|node| node.baseline == state.baseline)
                    .map(|node| (node.original, node.content, node.metadata))
            } else {
                None
            }
        };
        if let Some((original, content, metadata)) = cached {
            let attr = match active.get(&inode_key(original.serial))? {
                Some(value) => HotInode::parse(&value)?.attributes(original)?,
                None => original,
            };
            return Ok(Resolved {
                original,
                attr,
                content,
                metadata,
                canonical: true,
                base: view.base,
            });
        }
        let response = self.inspect_view(
            operation,
            view.base,
            Inspect::ChildAttributes {
                parent,
                name: name.to_vec(),
            },
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
        serial: u64,
        after: &[u8],
        limit: usize,
        deadline: Instant,
    ) -> Result<Vec<(Vec<u8>, u64)>, WorkspaceError> {
        if view.root.is_some() || limit == 0 || limit > 128 {
            return Err(WorkspaceError::InvalidInput);
        }
        let active = view.active.as_ref().ok_or(WorkspaceError::Io)?;
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
                        Inspect::InodeList {
                            serial,
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
                        check_name(name)?;
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
