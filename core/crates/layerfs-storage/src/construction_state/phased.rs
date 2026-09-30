//! Two fixed selected phases retained within one exact native owner.

use std::cell::Cell;

use layerfs_content::filesystem::state::{
    ClaimKey, ClaimSeal, StateScope, StateSelection, StateTable,
};

use crate::error::{StorageError, StorageResult};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub(crate) enum ClaimPhase {
    Open = 0,
    Sealed = 1,
    Retiring = 2,
    Retired = 3,
}

pub(crate) struct Phased {
    pub(crate) claims: StateScope,
    pub(crate) roots: StateScope,
    pub(crate) declared_roots: u64,
    pub(crate) declared_claims: u64,
    pub(crate) phase: ClaimPhase,
    pub(crate) records: u64,
    pub(crate) remaining: u64,
    pub(crate) maximum: Option<ClaimKey>,
    pub(crate) after: Option<ClaimKey>,
    pub(crate) seal: Option<ClaimSeal>,
    pub(crate) failed: Cell<bool>,
    pub(crate) attempt: Option<Attempt>,
    pub(crate) proposed_seal: Option<(ClaimSeal, Option<ClaimKey>)>,
}

/// The current unacknowledged bounded operation, retained on Unknown.
pub(crate) struct Attempt {
    pub(crate) kind: AttemptKind,
    pub(crate) before: u64,
    pub(crate) proposed: u64,
    pub(crate) serials: [u64; 128],
    pub(crate) count: usize,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum AttemptKind {
    Batch,
    Seal,
    Retire,
}

impl Attempt {
    pub(crate) fn description(&self) -> String {
        format!(
            "claim attempt {:?}: before={}, proposed={}, serials={:?}",
            self.kind,
            self.before,
            self.proposed,
            &self.serials[..self.count]
        )
    }
}

impl Phased {
    pub(crate) fn new(
        selection: &StateSelection,
        declared_roots: u64,
        declared_claims: u64,
    ) -> StorageResult<Self> {
        Ok(Self {
            claims: StateScope::new(selection.clone(), 1, StateTable::BindingClaims)?,
            roots: StateScope::new(selection.clone(), 2, StateTable::DirectoryRoots)?,
            declared_roots,
            declared_claims,
            phase: ClaimPhase::Open,
            records: 0,
            remaining: 0,
            maximum: None,
            after: None,
            seal: None,
            failed: Cell::new(false),
            attempt: None,
            proposed_seal: None,
        })
    }

    pub(crate) fn record_attempt(&mut self, kind: AttemptKind, keys: &[ClaimKey], proposed: u64) {
        let mut serials = [0; 128];
        for (slot, key) in serials.iter_mut().zip(keys) {
            *slot = key.serial();
        }
        self.attempt = Some(Attempt {
            kind,
            before: self.records,
            proposed,
            serials,
            count: keys.len(),
        });
    }

    pub(crate) fn check_claims(&self, scope: &StateScope, phase: ClaimPhase) -> StorageResult<()> {
        if scope != &self.claims || self.phase != phase || self.failed.get() {
            return Err(StorageError::Integrity("construction scratch claims phase"));
        }
        Ok(())
    }

    pub(crate) fn check_roots(&self, scope: &StateScope) -> StorageResult<()> {
        if scope != &self.roots || self.phase != ClaimPhase::Retired || self.failed.get() {
            return Err(StorageError::Integrity(
                "construction scratch claims not retired",
            ));
        }
        Ok(())
    }

    pub(crate) fn check_seal(&self, seal: &ClaimSeal, phase: ClaimPhase) -> StorageResult<()> {
        self.check_claims(seal.scope(), phase)?;
        if self.seal.as_ref() != Some(seal) {
            return Err(StorageError::Integrity(
                "construction scratch exact claim seal",
            ));
        }
        Ok(())
    }
}
