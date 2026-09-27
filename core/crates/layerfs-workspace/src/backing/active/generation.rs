use super::{
    extents::ExtentPlan,
    index::{Index, IndexSnapshot},
    pack::{PackedSlot, TinyPack},
    page::PageRef,
    pages::{PageStore, StoreStatus},
    reclaim::{self, RetiredPack},
};
use crate::{
    backing::{budget::Charge, directory::Directory, metadata::MetadataHost},
    WorkspaceError,
};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};

struct State {
    retired: Vec<RetiredPack>,
    retired_charge: Charge,
    stopped: bool,
    closed: bool,
}

/// Coordinates one Workspace's pack tail and pooled index. The index root is
/// the publication point; a prepared pack page is verified before it is named.
pub struct ActiveBacking {
    store: Arc<PageStore>,
    pack: Arc<TinyPack>,
    index: Arc<Index>,
    state: Mutex<State>,
    _charge: Charge,
}

pub struct ActiveSnapshot {
    active: Arc<ActiveBacking>,
    index: Option<IndexSnapshot>,
    pub generation: u64,
    pub tail: Option<(u64, PageRef)>,
}

#[derive(Clone, Debug)]
pub struct ActiveWrite {
    pub slot: PackedSlot,
    pub length: u64,
    pub revision: u64,
    /// Publication succeeded; cleanup failure remains owned and stops further
    /// admission until the caller handles it. It cannot change accepted bytes.
    pub cleanup_error: Option<WorkspaceError>,
}

#[derive(Clone, Copy, Debug)]
pub struct ActiveStatus {
    pub store: StoreStatus,
    pub retired_pack_pages: usize,
    pub stopped: bool,
    pub closed: bool,
}

pub(super) fn locator_key(logical: u64) -> Vec<u8> {
    [vec![b'P'], logical.to_be_bytes().to_vec()].concat()
}

fn locator_value(physical: PageRef) -> Vec<u8> {
    [physical.id.to_be_bytes(), physical.epoch.to_be_bytes()].concat()
}

pub(super) fn parse_locator(bytes: &[u8]) -> Result<PageRef, WorkspaceError> {
    if bytes.len() != 16 {
        return Err(WorkspaceError::Io);
    }
    let id = u64::from_be_bytes(bytes[..8].try_into().map_err(|_| WorkspaceError::Io)?);
    let epoch = u64::from_be_bytes(bytes[8..].try_into().map_err(|_| WorkspaceError::Io)?);
    if id == 0 || epoch == 0 {
        return Err(WorkspaceError::Io);
    }
    Ok(PageRef { id, epoch })
}

impl ActiveBacking {
    pub fn new(
        directory: Arc<Directory>,
        host: Arc<MetadataHost>,
    ) -> Result<Arc<Self>, WorkspaceError> {
        let store = PageStore::new(directory, host)?;
        let budget = store.budget();
        let pack = TinyPack::new(store.clone())?;
        let index = Index::new(store.clone())?;
        Ok(Arc::new(Self {
            store,
            pack,
            index,
            state: Mutex::new(State {
                retired: Vec::new(),
                retired_charge: budget.reserve(0)?,
                stopped: false,
                closed: false,
            }),
            _charge: budget.reserve(1024)?,
        }))
    }

    pub fn status(&self) -> Result<ActiveStatus, WorkspaceError> {
        let state = self.state.lock().map_err(|_| WorkspaceError::Io)?;
        Ok(ActiveStatus {
            store: self.store.status()?,
            retired_pack_pages: state.retired.len(),
            stopped: state.stopped,
            closed: state.closed,
        })
    }

    /// Reads one slot from the selected generation's locator root.
    pub fn read(
        &self,
        slot: PackedSlot,
        snapshot: Option<&ActiveSnapshot>,
    ) -> Result<Vec<u8>, WorkspaceError> {
        let _state = self.state.lock().map_err(|_| WorkspaceError::Io)?;
        let locator = match snapshot {
            Some(snapshot) => snapshot
                .index
                .as_ref()
                .ok_or(WorkspaceError::Closed)?
                .get(&locator_key(slot.logical_page))?,
            None => self.index.get(&locator_key(slot.logical_page))?,
        }
        .ok_or(WorkspaceError::NotFound)?;
        self.pack.read(slot, parse_locator(&locator)?)
    }

    /// One tiny data and index publication. Extra keyed updates supply inode,
    /// attribute, namespace and dirty records from the Workspace mutation.
    pub fn write_tiny(
        &self,
        inode: u64,
        old_length: u64,
        offset: u64,
        data: &[u8],
        extra: &[(Vec<u8>, Option<Vec<u8>>)],
    ) -> Result<ActiveWrite, WorkspaceError> {
        let mut state = self.state.lock().map_err(|_| WorkspaceError::Io)?;
        if state.stopped || state.closed {
            return Err(WorkspaceError::Busy);
        }
        let _working = self.store.budget().reserve(256 * 1024)?;
        self.index.maintain()?;
        let (generation, prior_revision) = self.index.generation_revision()?;
        let revision = prior_revision
            .checked_add(1)
            .ok_or(WorkspaceError::Capacity)?;
        let prepared = self
            .pack
            .prepare(inode, generation, revision, offset, data)?;
        let slot = prepared.slot();
        let planned = match ExtentPlan::tiny(&self.index, inode, old_length, slot) {
            Ok(plan) => plan,
            Err(error) => return Err(prepared.abort().err().unwrap_or(error)),
        };
        let length = planned.length;
        let mut updates: BTreeMap<Vec<u8>, Option<Vec<u8>>> = planned.updates.into_iter().collect();
        let (logical, physical) = prepared.locator();
        updates.insert(locator_key(logical), Some(locator_value(physical)));
        for (key, value) in extra {
            if updates.insert(key.clone(), value.clone()).is_some() {
                return Err(prepared
                    .abort()
                    .err()
                    .unwrap_or(WorkspaceError::InvalidInput));
            }
        }
        let dead = match reclaim::prune_dead(&self.index, &mut updates) {
            Ok(dead) => dead,
            Err(error) => return Err(prepared.abort().err().unwrap_or(error)),
        };
        let updates: Vec<_> = updates.into_iter().collect();
        let candidate = match self.index.prepare(&updates) {
            Ok(candidate) => candidate,
            Err(error) => return Err(prepared.abort().err().unwrap_or(error)),
        };
        let published_revision = match candidate.publish() {
            Ok(revision) => revision,
            Err(error) => return Err(prepared.abort().err().unwrap_or(error)),
        };
        let rewritten = prepared.retired_physical();
        let mut cleanup_error = prepared.publish().err();
        for page in rewritten.into_iter().chain(dead.into_iter()) {
            let State {
                retired,
                retired_charge,
                ..
            } = &mut *state;
            if let Err(error) = reclaim::retire_pack(
                &self.store,
                &self.index,
                retired,
                retired_charge,
                page,
                generation,
            ) {
                cleanup_error.get_or_insert(error);
            }
        }
        if cleanup_error.is_some() {
            state.stopped = true;
        }
        Ok(ActiveWrite {
            slot,
            length,
            revision: published_revision,
            cleanup_error,
        })
    }

    pub fn capture(self: &Arc<Self>) -> Result<ActiveSnapshot, WorkspaceError> {
        let state = self.state.lock().map_err(|_| WorkspaceError::Io)?;
        if state.stopped || state.closed {
            return Err(WorkspaceError::Busy);
        }
        let snapshot = self.index.capture()?;
        let tail = match self.pack.seal() {
            Ok(tail) => tail,
            Err(error) => return Err(snapshot.release().err().unwrap_or(error)),
        };
        let generation = self.index.generation_revision()?.0 - 1;
        Ok(ActiveSnapshot {
            active: self.clone(),
            index: Some(snapshot),
            generation,
            tail,
        })
    }

    pub fn close_clean(&self) -> Result<(), WorkspaceError> {
        let mut state = self.state.lock().map_err(|_| WorkspaceError::Io)?;
        if state.closed || self.index.frozen_count()? > 0 {
            return Err(WorkspaceError::Busy);
        }
        if state.stopped {
            return Err(WorkspaceError::Io);
        }
        self.index.maintain()?;
        reclaim::maintain_pack(&self.store, &self.index, &mut state.retired)?;
        if !state.retired.is_empty() {
            return Err(WorkspaceError::Busy);
        }
        self.store.close()?;
        state.closed = true;
        Ok(())
    }
}

impl ActiveSnapshot {
    pub fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>, WorkspaceError> {
        self.index.as_ref().ok_or(WorkspaceError::Closed)?.get(key)
    }

    pub fn release(mut self) -> Result<(), WorkspaceError> {
        let mut state = self.active.state.lock().map_err(|_| WorkspaceError::Io)?;
        self.index.take().ok_or(WorkspaceError::Closed)?.release()?;
        reclaim::maintain_pack(&self.active.store, &self.active.index, &mut state.retired)
    }
}

impl Drop for ActiveSnapshot {
    fn drop(&mut self) {
        if self.index.is_some() {
            if let Ok(mut state) = self.active.state.lock() {
                state.stopped = true;
            }
        }
    }
}
