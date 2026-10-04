//! Bounded WAL headroom using the shared allocator; SQLite owns logical bytes.
use super::{allocation_owner::AllocationOwner, connection::AllocationRelease};
use crate::backend::records::BackendError;
use std::{
    path::{Path, PathBuf},
    sync::{Mutex, TryLockError},
};

pub(crate) struct WalAllocation {
    path: PathBuf,
    owner: Mutex<Option<AllocationOwner>>,
}
impl WalAllocation {
    pub(crate) fn new(database: &Path) -> Result<Self, BackendError> {
        let database = if database.is_absolute() {
            database.to_owned()
        } else {
            std::env::current_dir()
                .map_err(|e| BackendError::Filesystem(e.kind()))?
                .join(database)
        };
        let mut name = database.as_os_str().to_os_string();
        name.push("-wal");
        Ok(Self {
            path: PathBuf::from(name),
            owner: Mutex::new(None),
        })
    }
    pub(crate) fn before_pack(&self, capacity: usize) -> Result<(u64, u64), BackendError> {
        let mut owner = self.owner.try_lock().map_err(|e| match e {
            TryLockError::WouldBlock => BackendError::Busy,
            TryLockError::Poisoned(_) => BackendError::Unknown,
        })?;
        // BEGIN IMMEDIATE has established SQLite's WAL before any pack INSERT.
        // Capture once; replacement/disappearance later is a custody failure.
        if owner.is_none() {
            *owner = Some(AllocationOwner::open(&self.path, false)?);
        }
        owner
            .as_ref()
            .ok_or(BackendError::Integrity)?
            .before_pack(capacity)
    }
    pub(crate) fn check_custody(&self) -> Result<(), BackendError> {
        let owner = self.owner.try_lock().map_err(|e| match e {
            TryLockError::WouldBlock => BackendError::Busy,
            TryLockError::Poisoned(_) => BackendError::Unknown,
        })?;
        if let Some(owner) = owner.as_ref() {
            owner.check_custody()?;
        }
        Ok(())
    }
    pub(crate) fn release(&self) -> Result<Option<AllocationRelease>, BackendError> {
        let owner = self.owner.try_lock().map_err(|e| match e {
            TryLockError::WouldBlock => BackendError::Busy,
            TryLockError::Poisoned(_) => BackendError::Unknown,
        })?;
        owner.as_ref().map(AllocationOwner::release).transpose()
    }
}
