//! Pooled ordered index v2: one selected root plus one optional hot directory.
//!
//! Every mutation stages complete page versions and one matching directory
//! copy; the current selection changes only when the caller publishes its
//! pack and Workspace state. A hot target names a directory slot rather than a
//! physical page, so replacing a referenced node leaves ancestors unchanged.
use super::{
    hot_cursor::{BoundUpdates, Cache, Cursor, Seed, CURSORS, DESCRIPTOR_BYTES},
    hot_directory::{Directory, DIRECTORY_RECORDS},
    keyed::{Target, HOT_SLOTS, MAX_LEVEL},
    page::{Kind, PageRef},
    pages::{PageReservation, PageStore},
    resolve::Resolver,
    retirement::Retirement,
    splice::{Mutation, PageCause, Update},
};
use crate::{backing::budget::Charge, WorkspaceError};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};

const MAX_CAPTURES: usize = 32;
const MAX_PINNED_REVISIONS: usize = MAX_CAPTURES + 128;
const STATE_BYTES: usize = 8192;
pub type IndexEntry = (Vec<u8>, Vec<u8>);

/// One charged selected directory copy. Its charge lives exactly as long as
/// the copy, so a frozen view keeps resolving while an old directory is pinned
/// and the copy is released when the last holder drops it.
pub(super) struct SelectedDirectory {
    pub(super) page: PageRef,
    pub(super) value: Directory,
    _charge: Charge,
}

#[derive(Clone)]
pub(super) struct Selection {
    pub(super) root: Option<Target>,
    pub(super) directory: Option<Arc<SelectedDirectory>>,
    pub(super) height: u8,
}

impl Selection {
    fn empty() -> Self {
        Self {
            root: None,
            directory: None,
            height: 0,
        }
    }
}

pub(super) struct State {
    pub(super) selection: Selection,
    pub(super) generation: u64,
    revision: u64,
    pending: bool,
    stopped: bool,
    captured: usize,
    frozen: BTreeMap<u64, usize>,
    pub(super) retired: Retirement,
    cleanup_error: Option<WorkspaceError>,
    pub(super) cache: Cache,
    pub(super) cursors: Vec<Cursor>,
    pub(super) pack_bindings: Vec<(Vec<u8>, super::hot_cursor::Binding)>,
    slot_epochs: [u64; HOT_SLOTS],
}

/// A pooled, ordered on-disk index. Every mutation stages complete page
/// versions; the current selection changes only after the caller publishes its
/// matching pack and Workspace state tuple.
pub struct Index {
    pub(super) store: Arc<PageStore>,
    pub(super) state: Mutex<State>,
    _charge: Charge,
}

pub struct IndexCandidate {
    index: Arc<Index>,
    expected_revision: u64,
    generation: u64,
    root: Option<Target>,
    height: u8,
    directory: Option<Arc<SelectedDirectory>>,
    directory_changed: bool,
    created: Vec<PageRef>,
    replaced: Vec<PageRef>,
    slot_epochs: [u64; HOT_SLOTS],
    cache: Cache,
    cursors: Vec<Cursor>,
    pack_bindings: Vec<(Vec<u8>, super::hot_cursor::Binding)>,
    _scratch: Charge,
    finished: bool,
}

pub struct IndexSnapshot {
    index: Arc<Index>,
    selection: Selection,
    revision: u64,
    captured: bool,
    released: bool,
}

pub struct ScanPage {
    entries: Vec<IndexEntry>,
    _charge: Charge,
}

impl ScanPage {
    pub fn entries(&self) -> &[IndexEntry] {
        &self.entries
    }
}

impl Drop for IndexCandidate {
    fn drop(&mut self) {
        if !self.finished {
            if let Ok(mut state) = self.index.state.lock() {
                state.stopped = true;
            }
        }
    }
}

impl Drop for IndexSnapshot {
    fn drop(&mut self) {
        if !self.released {
            if let Ok(mut state) = self.index.state.lock() {
                state.stopped = true;
            }
        }
    }
}

impl Index {
    pub(super) fn ready_for_retry(&self) -> Result<(), WorkspaceError> {
        let state = self.state.lock().map_err(|_| WorkspaceError::Io)?;
        if state.stopped || state.pending {
            return Err(WorkspaceError::Busy);
        }
        Ok(())
    }

    pub(super) fn hot_status(&self) -> Result<(usize, usize, usize, usize), WorkspaceError> {
        let state = self.state.lock().map_err(|_| WorkspaceError::Io)?;
        Ok((
            state
                .selection
                .directory
                .as_ref()
                .map_or(0, |directory| directory.value.occupied()),
            state.cursors.len(),
            super::hot_cursor::resident(&state.cache),
            state.retired.len(),
        ))
    }
    pub fn new(store: Arc<PageStore>) -> Result<Arc<Self>, WorkspaceError> {
        let budget = store.budget();
        Ok(Arc::new(Self {
            store,
            state: Mutex::new(State {
                selection: Selection::empty(),
                generation: 1,
                revision: 0,
                pending: false,
                stopped: false,
                captured: 0,
                frozen: BTreeMap::new(),
                retired: Retirement::new(&budget)?,
                cleanup_error: None,
                cache: std::array::from_fn(|_| None),
                cursors: Vec::new(),
                pack_bindings: Vec::new(),
                slot_epochs: [0; HOT_SLOTS],
            }),
            _charge: budget.reserve(STATE_BYTES + MAX_PINNED_REVISIONS * 128 + DESCRIPTOR_BYTES)?,
        }))
    }

    pub(crate) fn budget(&self) -> Arc<crate::backing::budget::Budget> {
        self.store.budget()
    }

    pub fn generation_revision(&self) -> Result<(u64, u64), WorkspaceError> {
        let state = self.state.lock().map_err(|_| WorkspaceError::Io)?;
        Ok((state.generation, state.revision))
    }

    pub fn frozen_between(&self, birth: u64, retired: u64) -> Result<bool, WorkspaceError> {
        let state = self.state.lock().map_err(|_| WorkspaceError::Io)?;
        Ok(state.frozen.range(birth..retired).next().is_some())
    }

    pub(super) fn selecting_revision(
        &self,
        birth: u64,
        retired: u64,
    ) -> Result<Option<u64>, WorkspaceError> {
        let state = self.state.lock().map_err(|_| WorkspaceError::Io)?;
        Ok(state
            .frozen
            .range(birth..retired)
            .next_back()
            .map(|(revision, _)| *revision))
    }

    pub(super) fn pin_exists(&self, revision: u64) -> Result<bool, WorkspaceError> {
        Ok(self
            .state
            .lock()
            .map_err(|_| WorkspaceError::Io)?
            .frozen
            .contains_key(&revision))
    }

    pub fn frozen_count(&self) -> Result<usize, WorkspaceError> {
        Ok(self
            .state
            .lock()
            .map_err(|_| WorkspaceError::Io)?
            .frozen
            .len())
    }

    pub fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>, WorkspaceError> {
        let state = self.state.lock().map_err(|_| WorkspaceError::Io)?;
        Resolver::cached(&self.store, &state.selection, &state.cache).get(key)
    }

    pub fn floor(&self, key: &[u8]) -> Result<Option<IndexEntry>, WorkspaceError> {
        let state = self.state.lock().map_err(|_| WorkspaceError::Io)?;
        Resolver::cached(&self.store, &state.selection, &state.cache).floor(key)
    }

    pub fn scan(
        &self,
        lower: &[u8],
        upper: &[u8],
        limit: usize,
    ) -> Result<ScanPage, WorkspaceError> {
        let state = self.state.lock().map_err(|_| WorkspaceError::Io)?;
        let charge = self.store.budget().reserve(64 * 1024 + limit * 1024)?;
        let entries = Resolver::cached(&self.store, &state.selection, &state.cache)
            .scan(lower, upper, limit)?;
        Ok(ScanPage {
            entries,
            _charge: charge,
        })
    }

    /// Sorted unique updates share one reached-subtree traversal and one
    /// candidate selection. A clean Commit may publish an empty update to
    /// advance its revision.
    pub fn prepare(self: &Arc<Self>, updates: &[Update]) -> Result<IndexCandidate, WorkspaceError> {
        self.prepare_file(updates, None, None, None)
    }

    pub(super) fn prepare_file(
        self: &Arc<Self>,
        updates: &[Update],
        seed: Option<Seed>,
        direct: Option<BoundUpdates>,
        reservation: Option<PageReservation>,
    ) -> Result<IndexCandidate, WorkspaceError> {
        if updates.windows(2).any(|pair| pair[0].0 >= pair[1].0)
            || updates.iter().any(|(key, value)| {
                key.is_empty()
                    || key.len() > super::keyed::MAX_KEY
                    || value
                        .as_ref()
                        .is_some_and(|value| value.len() > super::keyed::MAX_VALUE)
            })
        {
            return Err(WorkspaceError::InvalidInput);
        }
        let mut scratch = self.store.budget().reserve(128 * 1024)?;
        self.maintain()?;
        let (
            selection,
            generation,
            revision,
            epochs,
            cache,
            mut cursors,
            mut pack_bindings,
            captured,
            frozen,
        ) = {
            let mut state = self.state.lock().map_err(|_| WorkspaceError::Io)?;
            if state.pending || state.stopped {
                return Err(WorkspaceError::Busy);
            }
            state
                .revision
                .checked_add(1)
                .ok_or(WorkspaceError::Capacity)?;
            state.pending = true;
            (
                state.selection.clone(),
                state.generation,
                state.revision,
                state.slot_epochs,
                state.cache.clone(),
                state.cursors.clone(),
                state.pack_bindings.clone(),
                state.captured,
                state.frozen.len(),
            )
        };
        let next = revision + 1;
        let prior_cursor = seed
            .as_ref()
            .and_then(|seed| cursors.iter().find(|cursor| cursor.inode == seed.inode))
            .cloned();
        cursors.retain(|cursor| {
            !updates
                .iter()
                .any(|(key, _)| key.as_slice() == super::records::inode_key(cursor.inode))
        });
        if seed.is_some() && cursors.len() == CURSORS {
            cursors.remove(0);
        }
        let mut mutation =
            Mutation::new(&selection, epochs, self.store.budget().reserve(0)?, cache)?;
        let allow_cold = seed.is_some() && reservation.is_some();
        mutation.reservation = reservation;
        let staged = (|| {
            let budget = updates
                .iter()
                .try_fold(128 * 1024usize, |bytes, (key, value)| {
                    bytes
                        .checked_add(128 + key.len() + value.as_ref().map_or(0, Vec::len))
                        .ok_or(WorkspaceError::Capacity)
                })?;
            if updates.len() > 128
                && std::env::var_os("LFS_CAPACITY_DIAGNOSTIC").as_deref()
                    == Some(std::ffi::OsStr::new("1"))
            {
                eprintln!(
                    "LFS_INDEX_SCRATCH v=1 generation={} revision={} updates={} key_bytes={} value_bytes={} scratch_prior={} scratch_target={} budget_used={} captures={} frozen_revisions={}",
                    generation, next, updates.len(),
                    updates.iter().map(|(key, _)| key.len()).sum::<usize>(),
                    updates.iter().map(|(_, value)| value.as_ref().map_or(0, Vec::len)).sum::<usize>(),
                    scratch.bytes(), budget, self.store.budget().used(), captured, frozen,
                );
            }
            scratch.resize(budget)?;
            let mut keys: Vec<_> = cursors
                .iter()
                .flat_map(|cursor| cursor.bindings.iter())
                .filter(|(_, binding)| {
                    !matches!(
                        binding.role,
                        super::hot_cursor::Role::Locator | super::hot_cursor::Role::Inverse
                    )
                })
                .map(|(key, _)| key.clone())
                .collect();
            if let Some(seed) = &seed {
                keys.extend(seed.keys().into_iter().map(|(key, _)| key));
            } else {
                keys.extend(pack_bindings.iter().map(|(key, _)| key.clone()));
            }
            keys.sort();
            keys.dedup();
            mutation.want(&keys);
            let mut children = if let Some(groups) = &direct {
                let children =
                    mutation.direct(&self.store, &selection, generation, next, groups)?;
                self.store.count(super::pages::Counter::HotWrite, 1);
                children
            } else {
                self.store.count(super::pages::Counter::Seek, 1);
                mutation.admit(&self.store, &selection, &keys, allow_cold)?;
                mutation.change(
                    &self.store,
                    &selection,
                    selection.root,
                    selection.height,
                    updates,
                    &[],
                    &[],
                    generation,
                    next,
                )?
            };
            let mut level = selection.height;
            let (root, height) = if children.is_empty() {
                (None, 0)
            } else if children.len() == 1 {
                (Some(children[0].target), level)
            } else {
                loop {
                    if level >= MAX_LEVEL {
                        return Err(WorkspaceError::Capacity);
                    }
                    level += 1;
                    let nodes = Mutation::branch_groups(children, &[])?;
                    children = mutation.emit(
                        &self.store,
                        generation,
                        next,
                        level,
                        None,
                        nodes,
                        &[],
                        PageCause::Height,
                    )?;
                    if children.len() == 1 {
                        break (Some(children[0].target), level);
                    }
                }
            };
            let directory = if mutation.directory_changed && mutation.directory.occupied() > 0 {
                let body = mutation.directory.encode()?;
                let (page, stored) = self.store.create_from(
                    Kind::HotDirectory,
                    generation,
                    next,
                    DIRECTORY_RECORDS,
                    &body,
                    mutation.reservation.as_mut(),
                )?;
                mutation.created.push(page);
                let value = Directory::decode(&stored, self.store.incarnation(), page)?;
                if value.occupied() != mutation.directory.occupied() {
                    return Err(WorkspaceError::Io);
                }
                Some(Arc::new(SelectedDirectory {
                    page,
                    value,
                    _charge: self.store.budget().reserve(
                        std::mem::size_of::<SelectedDirectory>() + 2 * std::mem::size_of::<usize>(),
                    )?,
                }))
            } else {
                None
            };
            let selected = Selection {
                root,
                height,
                directory: if mutation.directory_changed {
                    directory.clone()
                } else {
                    selection.directory.clone()
                },
            };
            for cursor in &mut cursors {
                let refreshed = cursor
                    .bindings
                    .iter()
                    .map(|(key, binding)| {
                        let present = matches!(
                            binding.role,
                            super::hot_cursor::Role::Inode
                                | super::hot_cursor::Role::Dirty
                                | super::hot_cursor::Role::Locator
                        );
                        Some((
                            key.clone(),
                            binding.refresh(
                                &self.store,
                                &selected,
                                &mutation.directory,
                                &mutation.cache,
                                key,
                                present,
                                binding.role,
                            )?,
                        ))
                    })
                    .collect::<Option<Vec<_>>>();
                if let Some(bindings) = refreshed {
                    cursor.bindings = bindings;
                }
            }
            if seed.is_none() {
                pack_bindings = pack_bindings
                    .iter()
                    .map(|(key, binding)| {
                        Some((
                            key.clone(),
                            binding.refresh(
                                &self.store,
                                &selected,
                                &mutation.directory,
                                &mutation.cache,
                                key,
                                binding.role == super::hot_cursor::Role::Locator,
                                binding.role,
                            )?,
                        ))
                    })
                    .collect::<Option<Vec<_>>>()
                    .unwrap_or_default();
            }
            if let Some(seed) = &seed {
                if let Some(mut cursor) = seed.cursor(
                    &self.store,
                    &selected,
                    &mutation.directory,
                    &mutation.cache,
                    prior_cursor.as_ref(),
                    &pack_bindings,
                ) {
                    if direct.is_none() {
                        self.store.count(super::pages::Counter::CursorAdmission, 1);
                    }
                    pack_bindings = cursor
                        .bindings
                        .iter()
                        .filter(|(_, binding)| {
                            matches!(
                                binding.role,
                                super::hot_cursor::Role::Locator | super::hot_cursor::Role::Inverse
                            )
                        })
                        .cloned()
                        .collect();
                    cursor.bindings.retain(|(_, binding)| {
                        !matches!(
                            binding.role,
                            super::hot_cursor::Role::Locator | super::hot_cursor::Role::Inverse
                        )
                    });
                    if cursors.len() == CURSORS {
                        cursors.remove(0);
                    }
                    cursors.push(cursor);
                }
            }
            let mut state = self.state.lock().map_err(|_| WorkspaceError::Io)?;
            state.retired.reserve(
                mutation.replaced.len()
                    + usize::from(mutation.directory_changed && selection.directory.is_some()),
            )?;
            Ok::<_, WorkspaceError>((root, height, directory))
        })();
        let (root, height, directory) = match staged {
            Ok(staged) => staged,
            Err(error) => {
                let mut cleanup = None;
                for page in mutation.created.iter().rev() {
                    if let Err(failure) = self.store.release(*page) {
                        cleanup = Some(failure);
                    }
                }
                let mut state = self.state.lock().map_err(|_| WorkspaceError::Io)?;
                state.pending = false;
                if cleanup.is_some() {
                    state.stopped = true;
                }
                return Err(cleanup.unwrap_or(error));
            }
        };
        if std::env::var_os("LFS_CAPACITY_DIAGNOSTIC").as_deref() == Some(std::ffi::OsStr::new("1"))
        {
            let c = mutation.page_causes;
            eprintln!(
                "LFS_INDEX_PAGE_CAUSE v=2 generation={} revision={} update_keys={} selected_height={} new_height={} captured={} frozen_revisions={} direct={} changed_leaf={} changed_parent={} direct_leaf={} direct_parent={} admission_no_key={} normalization_no_key={} connection_no_key={} height_pages={} index_pages={} directory_pages={} created_total={} replaced={} hot_before={} hot_after={} generic_split_events={} generic_split_pages={} direct_carries={}",
                generation, next, updates.len(), selection.height, height, captured, frozen,
                direct.is_some(), c[PageCause::ChangedLeaf as usize],
                c[PageCause::ChangedParent as usize], c[PageCause::DirectLeaf as usize],
                c[PageCause::DirectParent as usize], c[PageCause::AdmitNoKey as usize],
                c[PageCause::NormalizeNoKey as usize], c[PageCause::ConnectNoKey as usize],
                c[PageCause::Height as usize], c.iter().sum::<u64>(),
                u64::from(mutation.directory_changed && directory.is_some()),
                mutation.created.len(), mutation.replaced.len(),
                selection.directory.as_ref().map_or(0, |d| d.value.occupied()),
                mutation.directory.occupied(),
                mutation.generic_splits, mutation.generic_split_pages, mutation.direct_carries,
            );
        }
        Ok(IndexCandidate {
            index: self.clone(),
            expected_revision: revision,
            generation,
            root,
            height,
            directory,
            directory_changed: mutation.directory_changed,
            created: mutation.created,
            replaced: mutation.replaced,
            slot_epochs: mutation.slot_epochs,
            cache: mutation.cache,
            cursors,
            pack_bindings,
            _scratch: scratch,
            finished: false,
        })
    }

    fn pin(self: &Arc<Self>, capture: bool) -> Result<IndexSnapshot, WorkspaceError> {
        let mut state = self.state.lock().map_err(|_| WorkspaceError::Io)?;
        let revision = state.revision;
        if state.pending
            || state.stopped
            || (capture && state.captured == MAX_CAPTURES)
            || (state.frozen.len() == MAX_PINNED_REVISIONS && !state.frozen.contains_key(&revision))
        {
            return Err(WorkspaceError::Busy);
        }
        if capture {
            let next_generation = state
                .generation
                .checked_add(1)
                .ok_or(WorkspaceError::Capacity)?;
            let next_revision = state
                .revision
                .checked_add(1)
                .ok_or(WorkspaceError::Capacity)?;
            state.generation = next_generation;
            state.revision = next_revision;
            state.captured += 1;
        }
        *state.frozen.entry(revision).or_default() += 1;
        Ok(IndexSnapshot {
            index: self.clone(),
            selection: state.selection.clone(),
            revision,
            captured: capture,
            released: false,
        })
    }

    pub fn capture(self: &Arc<Self>) -> Result<IndexSnapshot, WorkspaceError> {
        self.pin(true)
    }

    pub fn pin_current(self: &Arc<Self>) -> Result<IndexSnapshot, WorkspaceError> {
        self.pin(false)
    }

    pub fn maintain(&self) -> Result<u64, WorkspaceError> {
        let mut state = self.state.lock().map_err(|_| WorkspaceError::Io)?;
        match state.cleanup_error.take() {
            Some(error) => Err(error),
            None => Ok(0),
        }
    }
}

impl IndexCandidate {
    pub fn abort(mut self) -> Result<(), WorkspaceError> {
        for page in self.created.iter().rev() {
            self.index.store.release(*page)?;
        }
        self.index
            .state
            .lock()
            .map_err(|_| WorkspaceError::Io)?
            .pending = false;
        self.finished = true;
        Ok(())
    }

    pub fn publish(mut self) -> Result<u64, WorkspaceError> {
        let mut state = self.index.state.lock().map_err(|_| WorkspaceError::Io)?;
        if !state.pending
            || state.revision != self.expected_revision
            || state.generation != self.generation
        {
            return Err(WorkspaceError::Busy);
        }
        let next = state
            .revision
            .checked_add(1)
            .ok_or(WorkspaceError::Capacity)?;
        let mut replaced = self.replaced.clone();
        if self.directory_changed {
            if let Some(old) = state.selection.directory.as_ref() {
                replaced.push(old.page);
            }
        }
        let directory = if self.directory_changed {
            self.directory.take()
        } else {
            state.selection.directory.clone()
        };
        state.selection = Selection {
            root: self.root,
            directory,
            height: self.height,
        };
        state.slot_epochs = self.slot_epochs;
        state.cache = std::mem::replace(&mut self.cache, std::array::from_fn(|_| None));
        state.cursors = std::mem::take(&mut self.cursors);
        state.pack_bindings = std::mem::take(&mut self.pack_bindings);
        state.revision = next;
        state.pending = false;
        self.finished = true;
        for page in replaced {
            let State {
                retired, frozen, ..
            } = &mut *state;
            if let Err(error) = retired.retire(&self.index.store, page, next, |birth, retire| {
                Ok(frozen
                    .range(birth..retire)
                    .next_back()
                    .map(|(revision, _)| *revision))
            }) {
                state.cleanup_error.get_or_insert(error);
                state.stopped = true;
            }
        }
        Ok(next)
    }
}

impl IndexSnapshot {
    pub(super) fn revision(&self) -> u64 {
        self.revision
    }
    pub fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>, WorkspaceError> {
        Resolver::new(&self.index.store, &self.selection).get(key)
    }

    pub fn floor(&self, key: &[u8]) -> Result<Option<IndexEntry>, WorkspaceError> {
        Resolver::new(&self.index.store, &self.selection).floor(key)
    }

    pub fn scan(
        &self,
        lower: &[u8],
        upper: &[u8],
        limit: usize,
    ) -> Result<ScanPage, WorkspaceError> {
        let charge = self
            .index
            .store
            .budget()
            .reserve(64 * 1024 + limit * 1024)?;
        let entries =
            Resolver::new(&self.index.store, &self.selection).scan(lower, upper, limit)?;
        Ok(ScanPage {
            entries,
            _charge: charge,
        })
    }

    pub fn root(&self) -> Option<PageRef> {
        self.selection.root.and_then(|root| root.cold_page().ok())
    }

    pub fn release(mut self) -> Result<(), WorkspaceError> {
        let mut state = self.index.state.lock().map_err(|_| WorkspaceError::Io)?;
        let count = state
            .frozen
            .get_mut(&self.revision)
            .ok_or(WorkspaceError::Io)?;
        *count -= 1;
        if *count == 0 {
            state.frozen.remove(&self.revision);
            let State {
                retired, frozen, ..
            } = &mut *state;
            if let Err(error) =
                retired.release_selector(&self.index.store, self.revision, |birth, retire| {
                    Ok(frozen
                        .range(birth..retire)
                        .next_back()
                        .map(|(revision, _)| *revision))
                })
            {
                state.stopped = true;
                self.released = true;
                return Err(error);
            }
        }
        if self.captured {
            state.captured -= 1;
        }
        self.released = true;
        drop(state);
        self.index.maintain()?;
        Ok(())
    }
}
