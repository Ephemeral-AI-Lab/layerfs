//! Unforgeable established process ownership and terminal limit validation.

use std::sync::atomic::{AtomicBool, Ordering};

use crate::error::{StorageError, StorageResult};

use super::{
    ffi, EngineBootstrapObservation, EngineObservation, EngineProfile, ENGINE_HEAP_LIMIT_BYTES,
};

/// Established same-process engine owner. Consumers borrow its cached singleton.
/// It has no public constructor, clone, limit setter, reset or shutdown operation.
/// Dropping a borrow changes no native engine configuration.
#[derive(Debug)]
pub struct EngineGuard {
    profile: EngineProfile,
    bootstrap: EngineBootstrapObservation,
    quarantined: AtomicBool,
}

impl EngineGuard {
    pub(super) fn new(
        _established: ffi::Established,
        profile: EngineProfile,
        bootstrap: EngineBootstrapObservation,
    ) -> Self {
        Self {
            profile,
            bootstrap,
            quarantined: AtomicBool::new(false),
        }
    }

    /// Actual linked provider and fixed established heap/probe profile.
    pub const fn profile(&self) -> &EngineProfile {
        &self.profile
    }
    /// Exclusive startup's native tracking/free evidence, including defaults.
    pub const fn bootstrap_observation(&self) -> &EngineBootstrapObservation {
        &self.bootstrap
    }
    /// True after an observed foreign limit change or failed native observation.
    pub fn is_quarantined(&self) -> bool {
        self.quarantined.load(Ordering::Acquire)
    }

    /// Validate the exact current hard limit before a participating required effect.
    /// A mismatch/status failure ends this owner; restoring a foreign condition
    /// never authorizes repair, a fresh bootstrap or further validated effects.
    /// Current counter reads are observations, not per-owner admission credits.
    pub fn validate(&self) -> StorageResult<EngineObservation> {
        if self.is_quarantined() {
            return Err(StorageError::Integrity("SQLite engine guard quarantined"));
        }
        let result = ffi::observe(self).and_then(|observation| {
            if observation.hard_heap_limit_bytes != ENGINE_HEAP_LIMIT_BYTES {
                return Err(StorageError::Integrity("SQLite engine hard limit changed"));
            }
            Ok(observation)
        });
        if result.is_err() {
            self.quarantined.store(true, Ordering::Release);
        }
        result
    }
}
