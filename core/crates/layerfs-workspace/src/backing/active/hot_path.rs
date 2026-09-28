//! Bounded EOF and Base/Zero frontier publication using selected bindings.
use super::{
    extents::{Extent, ExtentKind},
    generation::{locator_key, locator_value, ActiveBacking, ActiveWrite},
    hot_cursor::{value, BoundUpdates, Role, Seed},
    index::Index,
    keyed::{Node, TAG_COLD},
    pack::PackedSlot,
    page::{PageRef, BODY_BYTES},
    pages::PageReservation,
    records::{dirty_key, inode_key, HotInode},
    splice::{Mutation, Update},
};
use crate::{backing::budget::Charge, WorkspaceError};
use std::{
    collections::{BTreeMap, BTreeSet},
    time::{SystemTime, UNIX_EPOCH},
};

pub(super) fn updated(
    mut inode: HotInode,
    generation: u64,
    revision: u64,
    length: u64,
) -> Result<HotInode, WorkspaceError> {
    inode.length = length;
    inode.generation = generation;
    inode.revision = revision;
    inode.storage = 2;
    inode.inline = [None; 4];
    let time = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| WorkspaceError::Io)?;
    inode.seconds = i64::try_from(time.as_secs()).map_err(|_| WorkspaceError::Capacity)?;
    inode.nanos = time.subsec_nanos();
    Ok(inode)
}

struct Plan {
    updates: Vec<Update>,
    groups: BoundUpdates,
    seed: Seed,
    pages: usize,
    _charge: Charge,
}

impl Index {
    /// A restricted Base/Zero or EOF boundary has <=6 leaf paths. Include the
    /// bounded existing Hot closure, two versions per reached node, root,
    /// directory and pack. The quota reservation precedes physical staging.
    pub(super) fn boundary_reservation(
        &self,
        updates: &[Update],
    ) -> Result<Option<PageReservation>, WorkspaceError> {
        if updates.len() > 12
            || updates
                .iter()
                .any(|(key, value)| matches!(key.first(), Some(b'R' | b'L')) && value.is_none())
        {
            return Ok(None);
        }
        let mut state = self.state.lock().map_err(|_| WorkspaceError::Io)?;
        let _paths = self.store.budget().reserve(64 * 1024)?;
        let mut cold = BTreeSet::new();
        let mut leaves = BTreeSet::new();
        for (key, _) in updates {
            let Some(mut target) = state.selection.root else {
                break;
            };
            let mut level = state.selection.height;
            let mut resolver =
                super::resolve::Resolver::cached(&self.store, &state.selection, &state.cache);
            self.store.count(super::pages::Counter::Seek, 1);
            loop {
                let node = resolver.node(target, level)?;
                let page = if target.tag == TAG_COLD {
                    target.cold_page()?
                } else {
                    state
                        .selection
                        .directory
                        .as_ref()
                        .ok_or(WorkspaceError::Io)?
                        .value
                        .entry(target.hot_slot()?)
                        .ok_or(WorkspaceError::Io)?
                        .page
                };
                if target.tag == TAG_COLD {
                    cold.insert(page);
                }
                match node {
                    Node::Leaf(_) => {
                        leaves.insert(page);
                        break;
                    }
                    Node::Branch(children) => {
                        let at = children.partition_point(|child| {
                            !child.fence.is_empty() && child.fence.as_slice() <= key.as_slice()
                        });
                        target = children.get(at).ok_or(WorkspaceError::Io)?.target;
                        level = level.checked_sub(1).ok_or(WorkspaceError::Io)?;
                    }
                }
            }
        }
        if leaves.len() > 6 {
            return Ok(None);
        }
        let hot = state
            .selection
            .directory
            .as_ref()
            .map_or(0, |directory| directory.value.occupied());
        let pages = if state.selection.root.is_none() {
            2
        } else {
            2 * (hot + cold.len()) + 2 + usize::from(state.selection.height < 7)
        };
        if pages > 227 {
            return Ok(None);
        }
        if self.store.remaining_quota()? < (pages * 4096) as u64 {
            return Ok(None);
        }
        if let Err(error) = state.retired.reserve(pages - 1) {
            return if error == WorkspaceError::Capacity {
                Ok(None)
            } else {
                Err(error)
            };
        }
        match self.store.reserve_pages(pages) {
            Ok(reservation) => Ok(Some(reservation)),
            Err(WorkspaceError::Capacity) => Ok(None),
            Err(error) => Err(error),
        }
    }

    /// Retained facts are validated against the selected I/D leaves. Another
    /// file's revision or replacement of their shared leaf does not miss.
    pub(super) fn file_facts(
        &self,
        inode: u64,
    ) -> Result<(Option<HotInode>, bool), WorkspaceError> {
        let state = self.state.lock().map_err(|_| WorkspaceError::Io)?;
        if let Some(directory) = &state.selection.directory {
            if let Some(cursor) = state.cursors.iter().find(|cursor| cursor.inode == inode) {
                if let Some(selected) = cursor.selected(
                    &self.store,
                    &state.selection,
                    &directory.value,
                    &state.cache,
                    state.generation,
                ) {
                    if let Some((key, binding)) = cursor
                        .bindings
                        .iter()
                        .find(|(_, binding)| binding.role == Role::Dirty)
                    {
                        if value(
                            &self.store,
                            &state.selection,
                            &state.cache,
                            &directory.value,
                            key,
                            binding,
                        )
                        .as_deref()
                            == Some(&[1])
                        {
                            return Ok((Some(selected), true));
                        }
                    }
                }
            }
        }
        let mut resolver =
            super::resolve::Resolver::cached(&self.store, &state.selection, &state.cache);
        let selected = resolver
            .get(&inode_key(inode))?
            .map(|value| HotInode::parse(&value))
            .transpose()?;
        let dirty = super::resolve::Resolver::cached(&self.store, &state.selection, &state.cache)
            .get(&dirty_key(state.generation, inode))?
            .is_some();
        Ok((selected, dirty))
    }

    fn hot_plan(
        &self,
        inode: u64,
        original: HotInode,
        slot: PackedSlot,
        locator_present: bool,
    ) -> Result<Option<Plan>, WorkspaceError> {
        let state = self.state.lock().map_err(|_| WorkspaceError::Io)?;
        let Some(directory) = &state.selection.directory else {
            return Ok(None);
        };
        let Some(cursor) = state.cursors.iter().find(|cursor| cursor.inode == inode) else {
            return Ok(None);
        };
        let Some(selected) = cursor.selected(
            &self.store,
            &state.selection,
            &directory.value,
            &state.cache,
            state.generation,
        ) else {
            return Ok(None);
        };
        if selected.storage != 2 || selected.value()? != original.value()? {
            return Ok(None);
        }
        let Some((dirty, binding)) = cursor
            .bindings
            .iter()
            .find(|(_, binding)| binding.role == Role::Dirty)
        else {
            return Ok(None);
        };
        if value(
            &self.store,
            &state.selection,
            &state.cache,
            &directory.value,
            dirty,
            binding,
        )
        .as_deref()
            != Some(&[1])
        {
            return Ok(None);
        }
        let packed = Extent::packed(slot.offset, slot)?;
        let source = if slot.offset == selected.length {
            None
        } else {
            let Some(source) = cursor.source.filter(|source| {
                matches!(source.kind, ExtentKind::Base | ExtentKind::Zero)
                    && slot.offset >= cursor.frontier
                    && slot.offset >= source.start
                    && packed.end <= source.end
            }) else {
                return Ok(None);
            };
            Some(source)
        };
        let charge = self.store.budget().reserve(128 * 1024)?;
        let selected = updated(
            selected,
            slot.generation,
            slot.revision,
            selected.length.max(packed.end),
        )?;
        let mut updates = BTreeMap::new();
        let mut suffix = None;
        if let Some(source) = source {
            updates.insert(Extent::key(inode, source.start).to_vec(), None);
            if source.start < packed.start {
                let gap = source.cut(source.start, packed.start)?;
                updates.insert(
                    Extent::key(inode, gap.start).to_vec(),
                    Some(gap.value()?.to_vec()),
                );
            }
            if packed.end < source.end {
                let next = source.cut(packed.end, source.end)?;
                updates.insert(
                    Extent::key(inode, next.start).to_vec(),
                    Some(next.value()?.to_vec()),
                );
                suffix = Some(next);
            }
        }
        updates.insert(
            Extent::key(inode, packed.start).to_vec(),
            Some(packed.value()?.to_vec()),
        );
        let inverse = packed.inverse_key(inode).ok_or(WorkspaceError::Io)?;
        updates.insert(inverse.clone(), Some(vec![1]));
        let locator = locator_key(slot.logical_page);
        updates.insert(
            locator.clone(),
            Some(locator_value(PageRef { id: 1, epoch: 1 })),
        );
        updates.insert(inode_key(inode).to_vec(), Some(selected.value()?.to_vec()));
        let mut groups = BTreeMap::<usize, Vec<Update>>::new();
        let mut paths = BTreeMap::new();
        for (key, update) in &updates {
            let bindings = if matches!(key[0], b'P' | b'R') {
                &state.pack_bindings
            } else {
                &cursor.bindings
            };
            let present = key[0] == b'I' || (key[0] == b'P' && locator_present) || update.is_none();
            let Some((_, binding)) = bindings.iter().find(|(_, binding)| {
                binding.admits(
                    &self.store,
                    &state.selection,
                    &directory.value,
                    &state.cache,
                    key,
                    present,
                )
            }) else {
                return Ok(None);
            };
            groups
                .entry(binding.slot)
                .or_default()
                .push((key.clone(), update.clone()));
            paths.insert(binding.slot, binding.clone());
        }
        if groups.len() > if source.is_some() { 5 } else { 4 } {
            return Ok(None);
        }
        let mut split = false;
        let mut mask = 0u64;
        for (slot, updates) in &groups {
            let Node::Leaf(cells) = &state.cache[*slot].as_ref().ok_or(WorkspaceError::Io)?.node
            else {
                return Err(WorkspaceError::Io);
            };
            split |= Node::Leaf(Mutation::merge(cells.clone(), updates)?).body_len() > BODY_BYTES;
            mask |= 1 << slot;
            for (slot, _, _) in &paths[slot].ancestors {
                mask |= 1 << slot;
            }
        }
        let pages = if split {
            2 * mask.count_ones() as usize + usize::from(state.selection.height < 7) + 2
        } else {
            groups.len() + 2
        };
        if pages > 98 {
            return Err(WorkspaceError::Capacity);
        }
        Ok(Some(Plan {
            updates: updates.into_iter().collect(),
            groups: BoundUpdates {
                groups,
                bindings: paths,
            },
            seed: Seed {
                inode,
                selected,
                frontier: packed.end,
                source: suffix,
                locator,
                inverse,
            },
            pages,
            _charge: charge,
        }))
    }
}

impl ActiveBacking {
    pub fn file_facts(&self, inode: u64) -> Result<(Option<HotInode>, bool), WorkspaceError> {
        let _state = self.state.lock().map_err(|_| WorkspaceError::Io)?;
        self.index.file_facts(inode)
    }

    pub(super) fn try_hot_file(
        &self,
        inode: u64,
        original: HotInode,
        offset: u64,
        data: &[u8],
    ) -> Result<Option<ActiveWrite>, WorkspaceError> {
        let mut state = self.state.lock().map_err(|_| WorkspaceError::Io)?;
        if state.stopped || state.closed {
            return Err(WorkspaceError::Busy);
        }
        let (generation, prior) = self.index.generation_revision()?;
        let revision = prior.checked_add(1).ok_or(WorkspaceError::Capacity)?;
        let (slot, rewritten) =
            self.pack
                .preview(inode, generation, revision, offset, data.len())?;
        let Some(mut plan) = self
            .index
            .hot_plan(inode, original, slot, rewritten.is_some())?
        else {
            return Ok(None);
        };
        state.retired.reserve(usize::from(rewritten.is_some()))?;
        {
            let mut index = self.index.state.lock().map_err(|_| WorkspaceError::Io)?;
            index.retired.reserve(plan.pages - 1)?;
        }
        let mut reservation = self.store.reserve_pages(plan.pages)?;
        let prepared = self.pack.prepare_from(
            inode,
            generation,
            revision,
            offset,
            data,
            Some(&mut reservation),
        )?;
        if prepared.slot() != slot {
            return Err(prepared.abort().err().unwrap_or(WorkspaceError::Io));
        }
        let (logical, physical) = prepared.locator();
        for (key, value) in &mut plan.updates {
            if *key == locator_key(logical) {
                *value = Some(locator_value(physical));
            }
        }
        for updates in plan.groups.groups.values_mut() {
            for (key, value) in updates {
                if *key == locator_key(logical) {
                    *value = Some(locator_value(physical));
                }
            }
        }
        let candidate = match self.index.prepare_file(
            &plan.updates,
            Some(plan.seed.clone()),
            Some(plan.groups),
            Some(reservation),
        ) {
            Ok(candidate) => candidate,
            Err(error) => return Err(prepared.abort().err().unwrap_or(error)),
        };
        let published = match candidate.publish() {
            Ok(revision) => revision,
            Err(error) => return Err(prepared.abort().err().unwrap_or(error)),
        };
        let mut cleanup_error = prepared.publish().err();
        if let Some(page) = rewritten {
            if let Err(error) =
                state
                    .retired
                    .retire(&self.store, page, published, |birth, retire| {
                        self.index.selecting_revision(birth, retire)
                    })
            {
                cleanup_error.get_or_insert(error);
            }
        }
        if let Err(error) = self.index.maintain() {
            cleanup_error.get_or_insert(error);
        }
        if cleanup_error.is_some() {
            state.stopped = true;
        }
        Ok(Some(ActiveWrite {
            slot,
            length: plan.seed.selected.length,
            revision: published,
            cleanup_error,
            inode: Some(plan.seed.selected),
        }))
    }
}
