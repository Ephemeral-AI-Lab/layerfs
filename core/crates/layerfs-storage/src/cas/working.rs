//! Scoped actual-Store Save/index reservations and retained native custody.

use std::sync::{Arc, Mutex, OnceLock, Weak};

use crate::error::{StorageError, StorageResult};
use crate::policy::SAVE_SLOT_SPACE;

use super::owner::MutationOwner;
use super::working_prepared::PreparedSave;

/// Scoped Save/shared-index reservation ceiling, not global or physical memory.
pub const SAVE_WORKING_BYTES: usize = 176 * 1024 * 1024;
/// Prospective actual shared or private/publication index pair allowance.
pub const SAVE_INDEX_PAIR_BYTES: usize = 8 * 1024 * 1024;

/// Declared prospective Save class selected before connection/SQL effects.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SaveWorkingClass {
    /// Standard72MiB reservation; raw read/consumer fit remains a separate gate.
    Standard,
    /// Separately selected96MiB reservation for the declared singleton profile.
    Singleton,
}
impl SaveWorkingClass {
    /// Fixed byte reservation; no class is enlarged after an admission failure.
    pub const fn bytes(self) -> usize {
        match self {
            Self::Standard => 72 * 1024 * 1024,
            Self::Singleton => 96 * 1024 * 1024,
        }
    }
}

/// One active or retained real Save reservation observation.
#[derive(Clone, Debug)]
pub struct SaveWorkingOwnerStatus {
    /// Fixed local custody slot, distinct from persisted writer-slot authority.
    pub slot: usize,
    /// Captured prospective class.
    pub class: SaveWorkingClass,
    /// Exact reserved bytes retained with these owners.
    pub reserved_bytes: usize,
    /// Known acknowledged native Save ID; Unknown birth keeps None.
    pub save_id: Option<i64>,
    /// Owner moved into its authority after failure/Drop.
    pub retained: bool,
    /// Unknown SQL/native ownership forbids cleanup/refund.
    pub quarantined: bool,
    /// Original failure text; typed original returns through the owning call.
    pub failure: Option<String>,
}

/// Actual scoped reservation facts; no raw-reader/whole-process fit claim.
#[derive(Clone, Debug)]
pub struct SaveWorkingStatus {
    /// Fixed scoped ledger ceiling.
    pub limit_bytes: usize,
    /// Active/retained Save plus real shared-index reservations.
    pub reserved_bytes: usize,
    /// Shared-index credit still held by actual index owners.
    pub shared_index_bytes: usize,
    /// Actual pre-effect Save byte-grant refusals, independent of SQL slots.
    pub byte_refusals: u64,
    /// Last prospectively selected refused Save class size.
    pub last_refused_bytes: Option<usize>,
    /// Bounded exact Save-owner observations.
    pub owners: Vec<SaveWorkingOwnerStatus>,
    /// Compiled retained-capsule allocation, admitted before Save birth.
    pub retention_capsule_bytes: usize,
}

enum Slot {
    Empty,
    Active(SaveWorkingOwnerStatus),
    Retained {
        status: SaveWorkingOwnerStatus,
        _capsule: Box<Option<RetainedSave>>,
    },
}
struct State {
    reserved: usize,
    shared_indexes: usize,
    byte_refusals: u64,
    last_refused_bytes: Option<usize>,
    slots: Vec<Slot>,
}
pub(crate) struct Shared {
    _native: Arc<Mutex<()>>,
    state: Mutex<State>,
}
#[derive(Clone)]
pub(crate) struct WorkingAuthority(Arc<Shared>);
impl std::fmt::Debug for WorkingAuthority {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("WorkingAuthority")
            .field("identity", &Arc::as_ptr(&self.0))
            .finish_non_exhaustive()
    }
}
type Registry = Vec<(Weak<Mutex<()>>, Weak<Shared>)>;
static REGISTRY: OnceLock<Mutex<Registry>> = OnceLock::new();

pub(crate) enum RetainedSave {
    Prepared(PreparedSave),
    Acquired(MutationOwner),
}

impl WorkingAuthority {
    pub(crate) fn associate(native: &Arc<Mutex<()>>) -> StorageResult<Self> {
        let mut registry = REGISTRY
            .get_or_init(|| Mutex::new(Vec::new()))
            .lock()
            .map_err(|_| StorageError::Integrity("Save working authority registry"))?;
        registry.retain(|(_, owner)| owner.strong_count() != 0);
        for (key, owner) in registry.iter() {
            if key.upgrade().is_some_and(|key| Arc::ptr_eq(&key, native)) {
                if let Some(owner) = owner.upgrade() {
                    return Ok(Self(owner));
                }
            }
        }
        if registry.len() >= SAVE_SLOT_SPACE {
            return Err(StorageError::OwnershipUnavailable);
        }
        let mut slots = Vec::new();
        slots
            .try_reserve_exact(SAVE_SLOT_SPACE)
            .map_err(|_| StorageError::Integrity("Save working slots"))?;
        slots.resize_with(SAVE_SLOT_SPACE, || Slot::Empty);
        let owner = Arc::new(Shared {
            _native: native.clone(),
            state: Mutex::new(State {
                reserved: 0,
                shared_indexes: 0,
                byte_refusals: 0,
                last_refused_bytes: None,
                slots,
            }),
        });
        registry.push((Arc::downgrade(native), Arc::downgrade(&owner)));
        Ok(Self(owner))
    }

    pub(crate) fn reserve_indexes(&self) -> StorageResult<Arc<IndexLease>> {
        let mut state = self
            .0
            .state
            .lock()
            .map_err(|_| StorageError::Integrity("Save working authority"))?;
        check(state.reserved, SAVE_INDEX_PAIR_BYTES)?;
        state.reserved += SAVE_INDEX_PAIR_BYTES;
        state.shared_indexes += SAVE_INDEX_PAIR_BYTES;
        Ok(Arc::new(IndexLease {
            shared: self.0.clone(),
        }))
    }

    pub(crate) fn reserve(&self, class: SaveWorkingClass) -> StorageResult<SaveLease> {
        let mut state = self
            .0
            .state
            .lock()
            .map_err(|_| StorageError::Integrity("Save working authority"))?;
        if state
            .reserved
            .checked_add(class.bytes())
            .is_none_or(|total| total > SAVE_WORKING_BYTES)
        {
            state.byte_refusals = state.byte_refusals.saturating_add(1);
            state.last_refused_bytes = Some(class.bytes());
            return Err(StorageError::OwnershipUnavailable);
        }
        let slot = state
            .slots
            .iter()
            .position(|slot| matches!(slot, Slot::Empty))
            .ok_or(StorageError::OwnershipUnavailable)?;
        state.reserved += class.bytes();
        state.slots[slot] = Slot::Active(SaveWorkingOwnerStatus {
            slot,
            class,
            reserved_bytes: class.bytes(),
            save_id: None,
            retained: false,
            quarantined: false,
            failure: None,
        });
        drop(state);
        Ok(SaveLease {
            shared: self.0.clone(),
            slot,
            class,
            capsule: Some(Box::new(None)),
        })
    }

    pub(crate) fn status(&self) -> StorageResult<SaveWorkingStatus> {
        let state = self
            .0
            .state
            .lock()
            .map_err(|_| StorageError::Integrity("Save working authority"))?;
        let mut owners = Vec::new();
        owners
            .try_reserve_exact(SAVE_SLOT_SPACE)
            .map_err(|_| StorageError::Integrity("Save working status"))?;
        for slot in &state.slots {
            match slot {
                Slot::Empty => {}
                Slot::Active(status) | Slot::Retained { status, .. } => owners.push(status.clone()),
            }
        }
        Ok(SaveWorkingStatus {
            limit_bytes: SAVE_WORKING_BYTES,
            reserved_bytes: state.reserved,
            shared_index_bytes: state.shared_indexes,
            byte_refusals: state.byte_refusals,
            last_refused_bytes: state.last_refused_bytes,
            owners,
            retention_capsule_bytes: std::mem::size_of::<Option<RetainedSave>>(),
        })
    }
}

fn check(current: usize, requested: usize) -> StorageResult<()> {
    if current
        .checked_add(requested)
        .is_none_or(|total| total > SAVE_WORKING_BYTES)
    {
        return Err(StorageError::CapacityExceeded {
            what: "C2 Save working bytes",
            limit: SAVE_WORKING_BYTES as u64,
            actual: current.saturating_add(requested) as u64,
        });
    }
    Ok(())
}

pub(crate) struct IndexLease {
    shared: Arc<Shared>,
}
impl std::fmt::Debug for IndexLease {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("IndexLease")
            .field("authority", &Arc::as_ptr(&self.shared))
            .field("bytes", &SAVE_INDEX_PAIR_BYTES)
            .finish()
    }
}
impl Drop for IndexLease {
    fn drop(&mut self) {
        let mut state = match self.shared.state.lock() {
            Ok(state) => state,
            Err(poisoned) => poisoned.into_inner(),
        };
        state.shared_indexes -= SAVE_INDEX_PAIR_BYTES;
        state.reserved -= SAVE_INDEX_PAIR_BYTES;
    }
}

pub(crate) struct SaveLease {
    shared: Arc<Shared>,
    slot: usize,
    class: SaveWorkingClass,
    capsule: Option<Box<Option<RetainedSave>>>,
}
impl SaveLease {
    pub(crate) fn acknowledged(&self, save_id: i64) {
        let mut state = match self.shared.state.lock() {
            Ok(state) => state,
            Err(poisoned) => poisoned.into_inner(),
        };
        let Slot::Active(status) = &mut state.slots[self.slot] else {
            unreachable!("exclusive acknowledged Save slot");
        };
        status.save_id = Some(save_id);
    }
    pub(crate) fn note(&self, failure: &StorageError, quarantined: bool) {
        let mut state = match self.shared.state.lock() {
            Ok(state) => state,
            Err(poisoned) => poisoned.into_inner(),
        };
        if let Slot::Active(status) = &mut state.slots[self.slot] {
            if status.failure.is_none() {
                status.failure = Some(failure.to_string());
            }
            status.quarantined |= quarantined;
        }
    }
}
impl Drop for SaveLease {
    fn drop(&mut self) {
        // Captured native/index/codec owners precede this field in real owners.
        drop(self.capsule.take());
        let mut state = match self.shared.state.lock() {
            Ok(state) => state,
            Err(poisoned) => poisoned.into_inner(),
        };
        if matches!(state.slots[self.slot], Slot::Active(_)) {
            state.slots[self.slot] = Slot::Empty;
            state.reserved -= self.class.bytes();
        }
    }
}

pub(crate) fn retain(mut value: RetainedSave) {
    let lease = match &mut value {
        RetainedSave::Prepared(value) => &mut value.working,
        RetainedSave::Acquired(value) => &mut value.working,
    };
    let shared = lease.shared.clone();
    let slot = lease.slot;
    let mut capsule = lease
        .capsule
        .take()
        .expect("one prepared Save retention capsule");
    *capsule = Some(value);
    let mut state = match shared.state.lock() {
        Ok(state) => state,
        Err(poisoned) => poisoned.into_inner(),
    };
    let prior = std::mem::replace(&mut state.slots[slot], Slot::Empty);
    let Slot::Active(mut status) = prior else {
        unreachable!("exclusive Save custody slot");
    };
    status.retained = true;
    state.slots[slot] = Slot::Retained {
        status,
        _capsule: capsule,
    };
}

const _: () = assert!(std::mem::size_of::<Option<RetainedSave>>() <= 1024 * 1024);
