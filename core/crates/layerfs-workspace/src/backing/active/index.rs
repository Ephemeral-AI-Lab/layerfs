//! Pooled ordered index v2: one selected root plus one optional hot directory.
//!
//! Every mutation stages complete page versions and one matching directory
//! copy; the current selection changes only when the caller publishes its
//! pack and Workspace state. A hot target names a directory slot rather than a
//! physical page, so replacing a referenced node leaves ancestors unchanged.
use super::{
    hot_directory::{Directory, DIRECTORY_BODY, DIRECTORY_RECORDS},
    keyed::{Target, HOT_SLOTS, MAX_LEVEL},
    page::{Kind, PageRef},
    pages::PageStore,
    resolve::Resolver,
    splice::{Mutation, Update},
};
use crate::{backing::budget::Charge, WorkspaceError};
use std::{
    collections::BTreeMap,
    mem::size_of,
    sync::{Arc, Mutex},
};

const MAX_CAPTURES: usize = 32;
const MAX_PINNED_REVISIONS: usize = MAX_CAPTURES + 128;
const STATE_BYTES: usize = 8192;
pub type IndexEntry = (Vec<u8>, Vec<u8>);

struct Retired {
    page: PageRef,
    birth: u64,
    retired: u64,
}

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

struct State {
    selection: Selection,
    generation: u64,
    revision: u64,
    pending: bool,
    stopped: bool,
    captured: usize,
    frozen: BTreeMap<u64, usize>,
    retired: Vec<Retired>,
    retired_charge: Charge,
    slot_epochs: [u64; HOT_SLOTS],
}

/// A pooled, ordered on-disk index. Every mutation stages complete page
/// versions; the current selection changes only after the caller publishes its
/// matching pack and Workspace state tuple.
pub struct Index {
    store: Arc<PageStore>,
    state: Mutex<State>,
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
                retired: Vec::new(),
                retired_charge: budget.reserve(0)?,
                slot_epochs: [0; HOT_SLOTS],
            }),
            _charge: budget.reserve(STATE_BYTES + MAX_PINNED_REVISIONS * 128)?,
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
        Resolver::new(&self.store, &state.selection).get(key)
    }

    pub fn floor(&self, key: &[u8]) -> Result<Option<IndexEntry>, WorkspaceError> {
        let state = self.state.lock().map_err(|_| WorkspaceError::Io)?;
        Resolver::new(&self.store, &state.selection).floor(key)
    }

    pub fn scan(
        &self,
        lower: &[u8],
        upper: &[u8],
        limit: usize,
    ) -> Result<ScanPage, WorkspaceError> {
        let state = self.state.lock().map_err(|_| WorkspaceError::Io)?;
        let charge = self.store.budget().reserve(64 * 1024 + limit * 1024)?;
        let entries = Resolver::new(&self.store, &state.selection).scan(lower, upper, limit)?;
        Ok(ScanPage {
            entries,
            _charge: charge,
        })
    }

    /// Sorted unique updates share one reached-subtree traversal and one
    /// candidate selection. A clean Commit may publish an empty update to
    /// advance its revision.
    pub fn prepare(self: &Arc<Self>, updates: &[Update]) -> Result<IndexCandidate, WorkspaceError> {
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
        let (selection, generation, revision, epochs) = {
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
            )
        };
        let next = revision + 1;
        let mut mutation = Mutation::new(&selection, epochs, self.store.budget().reserve(0)?)?;
        let staged = (|| {
            let budget = updates
                .iter()
                .try_fold(128 * 1024usize, |bytes, (key, value)| {
                    bytes
                        .checked_add(128 + key.len() + value.as_ref().map_or(0, Vec::len))
                        .ok_or(WorkspaceError::Capacity)
                })?;
            scratch.resize(budget)?;
            let mut children = mutation.change(
                &self.store,
                &selection,
                selection.root,
                selection.height,
                updates,
                &[],
                generation,
                next,
            )?;
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
                    children = mutation.emit(&self.store, generation, next, level, None, nodes)?;
                    if children.len() == 1 {
                        break (Some(children[0].target), level);
                    }
                }
            };
            let directory = if mutation.directory_changed {
                let body = mutation.directory.encode()?;
                let (page, stored) = self.store.create_verified(
                    Kind::HotDirectory,
                    generation,
                    next,
                    DIRECTORY_RECORDS,
                    &body,
                )?;
                mutation.created.push(page);
                let value = Directory::decode(&stored, self.store.incarnation(), page)?;
                if value.occupied() != mutation.directory.occupied() {
                    return Err(WorkspaceError::Io);
                }
                Some(Arc::new(SelectedDirectory {
                    page,
                    value,
                    _charge: self.store.budget().reserve(DIRECTORY_BODY)?,
                }))
            } else {
                None
            };
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
        let mut released = 0;
        loop {
            let selected = {
                let mut state = self.state.lock().map_err(|_| WorkspaceError::Io)?;
                let at = state.retired.iter().position(|item| {
                    state
                        .frozen
                        .range(item.birth..item.retired)
                        .next()
                        .is_none()
                });
                at.map(|at| state.retired.swap_remove(at))
            };
            let Some(item) = selected else { break };
            match self.store.release(item.page) {
                Ok(bytes) => released += bytes,
                Err(error) => {
                    let mut state = self.state.lock().map_err(|_| WorkspaceError::Io)?;
                    state.retired.push(item);
                    state.stopped = true;
                    return Err(error);
                }
            }
        }
        Ok(released)
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
        let mut retire = Vec::with_capacity(replaced.len());
        for page in replaced {
            let stored = self.index.store.read(page, self.index.store.kind(page)?)?;
            retire.push(Retired {
                page,
                birth: stored.revision(),
                retired: next,
            });
        }
        let growth = state
            .retired
            .len()
            .checked_add(retire.len())
            .and_then(|entries| entries.checked_mul(size_of::<Retired>()))
            .and_then(|bytes| bytes.checked_mul(2))
            .ok_or(WorkspaceError::Capacity)?;
        let required = growth.max(state.retired.capacity() * size_of::<Retired>());
        state.retired_charge.resize(required)?;
        state
            .retired
            .try_reserve_exact(retire.len())
            .map_err(|_| WorkspaceError::Capacity)?;
        let actual = state.retired.capacity() * size_of::<Retired>();
        state.retired_charge.resize(actual.max(required))?;
        state.retired.extend(retire);
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
        state.revision = next;
        state.pending = false;
        self.finished = true;
        Ok(next)
    }
}

impl IndexSnapshot {
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
