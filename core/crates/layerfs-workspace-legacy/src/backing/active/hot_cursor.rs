//! Charged selected nodes and bounded inode/frontier descriptors.
use super::{
    extents::Extent,
    hot_directory::Directory,
    index::Selection,
    keyed::{Node, Target, HOT_SLOTS, MAX_LEVEL},
    page::{Page, PageRef, PAGE_BYTES},
    pages::{Cause, Counter, PageStore},
    records::{dirty_key, inode_key, HotInode},
};
use crate::{backing::budget::Charge, WorkspaceError};
use std::{collections::BTreeMap, mem::size_of, sync::Arc};

pub(super) const CURSORS: usize = 8;
pub(super) const DESCRIPTOR_BYTES: usize = 64 * 1024;
pub(super) const RESIDENT_BYTES: usize = 1024 * 1024;
pub(super) type Cache = [Option<Arc<Cached>>; HOT_SLOTS];

pub(super) struct BoundUpdates {
    pub(super) groups: BTreeMap<usize, Vec<super::splice::Update>>,
    pub(super) bindings: BTreeMap<usize, Binding>,
}

pub(super) struct Cached {
    pub(super) page: PageRef,
    pub(super) node: Node,
    pub(super) bytes: usize,
    _charge: Charge,
}

impl Cached {
    /// The same prepared Node whose encoded body was used to create this
    /// authenticated private page. `create_from` verifies its exact on-disk
    /// readback before returning; do not allocate and decode its cells again.
    pub(super) fn prepared(
        store: &PageStore,
        page: PageRef,
        stored: &Page,
        node: Node,
        encoded_bytes: usize,
    ) -> Result<Arc<Self>, WorkspaceError> {
        if store.kind(page)? != node.kind() || stored.records() as usize != node.len() {
            return Err(WorkspaceError::Io);
        }
        let bytes =
            PAGE_BYTES + encoded_bytes * 2 + stored.records() as usize * 64 + size_of::<Self>();
        let charge = store.budget().reserve(bytes)?;
        Ok(Arc::new(Self {
            page,
            node,
            bytes,
            _charge: charge,
        }))
    }

    pub(super) fn decode(
        store: &PageStore,
        page: PageRef,
        stored: &Page,
    ) -> Result<Arc<Self>, WorkspaceError> {
        let kind = store.kind(page)?;
        let body = stored.verify(kind, store.incarnation(), page)?;
        let bytes =
            PAGE_BYTES + body.len() * 2 + stored.records() as usize * 64 + size_of::<Self>();
        let charge = store.budget().reserve(bytes)?;
        let decode_time = store.stamp(Cause::CacheDecode);
        let node = Node::decode(stored, store.incarnation(), page, kind)?;
        drop(decode_time);
        Ok(Arc::new(Self {
            page,
            node,
            bytes,
            _charge: charge,
        }))
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Role {
    Inode,
    Dirty,
    Locator,
    Frontier,
    Inverse,
}

#[derive(Clone)]
pub(super) struct Binding {
    pub(super) role: Role,
    pub(super) slot: usize,
    pub(super) epoch: u64,
    pub(super) lower: Vec<u8>,
    pub(super) fence: Vec<u8>,
    pub(super) ancestors: Vec<(usize, u64, u64)>,
}

pub(super) fn cached<'a>(
    cache: &'a Cache,
    directory: &Directory,
    target: Target,
) -> Option<&'a Cached> {
    let slot = target.hot_slot().ok()?;
    let entry = directory.entry(slot)?;
    let node = cache[slot].as_deref()?;
    (entry.epoch == target.epoch && node.page == entry.page).then_some(node)
}

impl Binding {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn refresh(
        &self,
        store: &PageStore,
        selection: &Selection,
        directory: &Directory,
        cache: &Cache,
        key: &[u8],
        present: bool,
        role: Role,
    ) -> Option<Self> {
        if self.admits(store, selection, directory, cache, key, present) {
            let mut binding = self.clone();
            binding.role = role;
            return Some(binding);
        }
        let root = selection.root?;
        let expected = match self.ancestors.first() {
            Some((slot, epoch, _)) => Target::hot(*slot, *epoch).ok()?,
            None => Target::hot(self.slot, self.epoch).ok()?,
        };
        if root != expected {
            store.count(Counter::Seek, 1);
            return Self::capture_counted(store, selection, directory, cache, key, role);
        }
        let mut lower = Vec::new();
        let mut fence = Vec::new();
        let mut ancestors = Vec::new();
        for (position, (slot, epoch, version)) in self.ancestors.iter().enumerate() {
            let target = Target::hot(*slot, *epoch).ok()?;
            let node = cached(cache, directory, target)?;
            if node.page.id != *version {
                return Self::descend(
                    Some(store),
                    target,
                    directory,
                    cache,
                    key,
                    role,
                    lower,
                    fence,
                    ancestors,
                );
            }
            let Node::Branch(children) = &node.node else {
                return None;
            };
            let next = match self.ancestors.get(position + 1) {
                Some((slot, epoch, _)) => Target::hot(*slot, *epoch).ok()?,
                None => Target::hot(self.slot, self.epoch).ok()?,
            };
            let at = children.iter().position(|child| child.target == next)?;
            let child_lower = if at > 0 {
                children[at - 1].fence.clone()
            } else {
                lower.clone()
            };
            let child_fence = if children[at].fence.is_empty() {
                fence.clone()
            } else {
                children[at].fence.clone()
            };
            if key < child_lower.as_slice()
                || (!child_fence.is_empty() && key >= child_fence.as_slice())
            {
                return Self::descend(
                    Some(store),
                    target,
                    directory,
                    cache,
                    key,
                    role,
                    lower,
                    fence,
                    ancestors,
                );
            }
            lower = child_lower;
            fence = child_fence;
            ancestors.push((*slot, *epoch, node.page.id));
        }
        Self::descend(
            Some(store),
            Target::hot(self.slot, self.epoch).ok()?,
            directory,
            cache,
            key,
            role,
            lower,
            fence,
            ancestors,
        )
    }

    pub(super) fn admits(
        &self,
        store: &PageStore,
        selection: &Selection,
        directory: &Directory,
        cache: &Cache,
        key: &[u8],
        present: bool,
    ) -> bool {
        let expected_root = match self.ancestors.first() {
            Some((slot, epoch, _)) => Target::hot(*slot, *epoch).ok(),
            None => Target::hot(self.slot, self.epoch).ok(),
        };
        if selection.root != expected_root
            || key < self.lower.as_slice()
            || (!self.fence.is_empty() && key >= self.fence.as_slice())
            || self.ancestors.iter().any(|(slot, epoch, version)| {
                directory
                    .entry(*slot)
                    .is_none_or(|entry| entry.epoch != *epoch || entry.page.id != *version)
            })
        {
            return false;
        }
        let Some(entry) = directory
            .entry(self.slot)
            .filter(|entry| entry.epoch == self.epoch && entry.level == 0)
        else {
            return false;
        };
        let Some(node) = cache[self.slot]
            .as_deref()
            .filter(|node| node.page == entry.page)
        else {
            return false;
        };
        let Node::Leaf(cells) = &node.node else {
            return false;
        };
        store.count(Counter::Visit, 1);
        store.count(Counter::CacheHit, 1);
        !present
            || cells
                .binary_search_by(|cell| cell.key.as_slice().cmp(key))
                .is_ok()
    }

    fn capture_counted(
        store: &PageStore,
        selection: &Selection,
        directory: &Directory,
        cache: &Cache,
        key: &[u8],
        role: Role,
    ) -> Option<Self> {
        Self::descend(
            Some(store),
            selection.root?,
            directory,
            cache,
            key,
            role,
            Vec::new(),
            Vec::new(),
            Vec::new(),
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn descend(
        store: Option<&PageStore>,
        mut target: Target,
        directory: &Directory,
        cache: &Cache,
        key: &[u8],
        role: Role,
        mut lower: Vec<u8>,
        mut fence: Vec<u8>,
        mut ancestors: Vec<(usize, u64, u64)>,
    ) -> Option<Self> {
        for _ in 0..=MAX_LEVEL {
            if let Some(store) = store {
                store.count(Counter::Visit, 1);
            }
            let node = cached(cache, directory, target)?;
            let slot = target.hot_slot().ok()?;
            match &node.node {
                Node::Leaf(_) => {
                    return Some(Self {
                        role,
                        slot,
                        epoch: target.epoch,
                        lower,
                        fence,
                        ancestors,
                    })
                }
                Node::Branch(children) => {
                    ancestors.push((slot, target.epoch, node.page.id));
                    let at = children.partition_point(|child| {
                        !child.fence.is_empty() && child.fence.as_slice() <= key
                    });
                    let child = children.get(at)?;
                    if at > 0 {
                        lower = children[at - 1].fence.clone();
                    }
                    if !child.fence.is_empty() {
                        fence = child.fence.clone();
                    }
                    target = child.target;
                }
            }
        }
        None
    }
}

#[derive(Clone)]
pub(super) struct Cursor {
    pub(super) inode: u64,
    pub(super) selected: HotInode,
    pub(super) frontier: u64,
    pub(super) source: Option<Extent>,
    pub(super) bindings: Vec<(Vec<u8>, Binding)>,
}

impl Cursor {
    pub(super) fn selected(
        &self,
        store: &PageStore,
        selection: &Selection,
        directory: &Directory,
        cache: &Cache,
        generation: u64,
    ) -> Option<HotInode> {
        if self.selected.generation != generation {
            return None;
        }
        let key = inode_key(self.inode);
        let (_, binding) = self
            .bindings
            .iter()
            .find(|(_, binding)| binding.role == Role::Inode)?;
        if !binding.admits(store, selection, directory, cache, &key, true) {
            return None;
        }
        let Node::Leaf(cells) = &cache[binding.slot].as_ref()?.node else {
            return None;
        };
        let at = cells
            .binary_search_by(|cell| cell.key.as_slice().cmp(&key))
            .ok()?;
        let selected = HotInode::parse(&cells[at].value).ok()?;
        (selected.revision == self.selected.revision).then_some(selected)
    }
}

#[derive(Clone)]
pub(super) struct Seed {
    pub(super) inode: u64,
    pub(super) selected: HotInode,
    pub(super) frontier: u64,
    pub(super) source: Option<Extent>,
    pub(super) locator: Vec<u8>,
    pub(super) inverse: Vec<u8>,
}

impl Seed {
    pub(super) fn keys(&self) -> Vec<(Vec<u8>, Role)> {
        let mut keys = vec![
            (inode_key(self.inode).to_vec(), Role::Inode),
            (
                dirty_key(self.selected.generation, self.inode).to_vec(),
                Role::Dirty,
            ),
            (
                Extent::key(self.inode, self.frontier).to_vec(),
                Role::Frontier,
            ),
            (self.locator.clone(), Role::Locator),
            (self.inverse.clone(), Role::Inverse),
        ];
        if let Some(source) = self.source {
            keys.push((
                Extent::key(self.inode, source.start).to_vec(),
                Role::Frontier,
            ));
        }
        keys
    }

    pub(super) fn cursor(
        &self,
        store: &PageStore,
        selection: &Selection,
        directory: &Directory,
        cache: &Cache,
        prior: Option<&Cursor>,
        pack: &[(Vec<u8>, Binding)],
    ) -> Option<Cursor> {
        let bindings = self
            .keys()
            .into_iter()
            .map(|(key, role)| {
                let present = matches!(role, Role::Inode | Role::Dirty | Role::Locator);
                let old = if matches!(role, Role::Locator | Role::Inverse) {
                    pack
                } else {
                    prior.map_or(&[][..], |cursor| cursor.bindings.as_slice())
                };
                let binding = old
                    .iter()
                    .filter(|(_, binding)| binding.role == role)
                    .find_map(|(_, binding)| {
                        binding.refresh(store, selection, directory, cache, &key, present, role)
                    })
                    .or_else(|| {
                        store.count(Counter::Seek, 1);
                        Binding::capture_counted(store, selection, directory, cache, &key, role)
                    })?;
                if !binding.admits(store, selection, directory, cache, &key, present) {
                    return None;
                }
                Some((key, binding))
            })
            .collect::<Option<Vec<_>>>()?;
        Some(Cursor {
            inode: self.inode,
            selected: self.selected,
            frontier: self.frontier,
            source: self.source,
            bindings,
        })
    }
}

pub(super) fn resident(cache: &Cache) -> usize {
    DESCRIPTOR_BYTES + PAGE_BYTES * 3 + cache.iter().flatten().map(|node| node.bytes).sum::<usize>()
}

pub(super) fn value(
    store: &PageStore,
    selection: &Selection,
    cache: &Cache,
    directory: &Directory,
    key: &[u8],
    binding: &Binding,
) -> Option<Vec<u8>> {
    if !binding.admits(store, selection, directory, cache, key, true) {
        return None;
    }
    let Node::Leaf(cells) = &cache[binding.slot].as_ref()?.node else {
        return None;
    };
    let at = cells
        .binary_search_by(|cell| cell.key.as_slice().cmp(key))
        .ok()?;
    Some(cells[at].value.clone())
}
