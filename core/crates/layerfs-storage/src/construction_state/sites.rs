//! The fixed v3 phase owner and bounded unacknowledged site-operation capsule.

use std::cell::Cell;

use layerfs_content::filesystem::rows::BindingSourceId;
use layerfs_content::filesystem::state::{
    SiteBirthSeal, SiteKey, SiteMembership, SiteObservation, SiteRecord, SiteScope, SiteSeal,
    StateScope, StateSelection, StateTable,
};

use crate::error::{StorageError, StorageResult};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub(crate) enum SiteStage {
    BirthOpen = 0,
    Facts = 1,
    FinalSealed = 2,
    Retiring = 3,
    Retired = 4,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum SiteAttemptKind {
    Birth,
    CloseMembership,
    Observe,
    FinalSeal,
    Retire,
}

pub(crate) struct SiteAttempt {
    pub(crate) kind: SiteAttemptKind,
    pub(crate) before: u64,
    pub(crate) proposed: u64,
    pub(crate) records: [Option<SiteRecord>; 128],
    pub(crate) proposed_flags: [u8; 128],
    pub(crate) count: usize,
    pub(crate) observations: [Option<SiteObservation>; 128],
    pub(crate) observation_count: usize,
    pub(crate) prior_after: Option<SiteKey>,
    pub(crate) proposed_after: Option<SiteKey>,
}

impl SiteAttempt {
    pub(crate) fn new(kind: SiteAttemptKind, before: u64, proposed: u64) -> Self {
        Self {
            kind,
            before,
            proposed,
            records: [None; 128],
            proposed_flags: [0; 128],
            count: 0,
            observations: [None; 128],
            observation_count: 0,
            prior_after: None,
            proposed_after: None,
        }
    }

    pub(crate) fn include(&mut self, record: SiteRecord, proposed_flags: u8) {
        self.records[self.count] = Some(record);
        self.proposed_flags[self.count] = proposed_flags;
        self.count += 1;
    }
}

pub(crate) struct Sites {
    pub(crate) scope: SiteScope,
    pub(crate) roots: StateScope,
    pub(crate) declared_roots: u64,
    pub(crate) declared_sites: u64,
    pub(crate) stage: SiteStage,
    pub(crate) records: u64,
    pub(crate) remaining: u64,
    pub(crate) membership: Option<SiteMembership>,
    pub(crate) seal: Option<SiteSeal>,
    pub(crate) after: Option<SiteKey>,
    pub(crate) failed: Cell<bool>,
    pub(crate) attempt: Option<SiteAttempt>,
    pub(crate) expected_birth: Option<SiteBirthSeal>,
    pub(crate) proposed_membership: Option<SiteMembership>,
    pub(crate) proposed_seal: Option<(SiteSeal, Option<SiteKey>)>,
}

impl Sites {
    pub(crate) fn new(
        selection: &StateSelection,
        directories: u64,
        bindings: u64,
        source: BindingSourceId,
    ) -> StorageResult<Self> {
        Self::with_roots_phase(selection, directories, bindings, source, 2)
    }

    pub(crate) fn with_roots_phase(
        selection: &StateSelection,
        directories: u64,
        bindings: u64,
        source: BindingSourceId,
        roots_phase: u64,
    ) -> StorageResult<Self> {
        Ok(Self {
            scope: SiteScope::new(
                StateScope::new(selection.clone(), 1, StateTable::BindingSites)?,
                source,
            )?,
            roots: StateScope::new(selection.clone(), roots_phase, StateTable::DirectoryRoots)?,
            declared_roots: directories,
            declared_sites: bindings,
            stage: SiteStage::BirthOpen,
            records: 0,
            remaining: 0,
            membership: None,
            seal: None,
            after: None,
            failed: Cell::new(false),
            attempt: None,
            expected_birth: None,
            proposed_membership: None,
            proposed_seal: None,
        })
    }

    pub(crate) fn check(&self, scope: &SiteScope, stage: SiteStage) -> StorageResult<()> {
        if scope != &self.scope || self.stage != stage || self.failed.get() {
            return Err(StorageError::Integrity("construction scratch site phase"));
        }
        Ok(())
    }

    pub(crate) fn check_membership(&self, members: &SiteMembership) -> StorageResult<()> {
        self.check(members.birth().scope(), SiteStage::Facts)?;
        if self.membership.as_ref() != Some(members) {
            return Err(StorageError::Integrity(
                "construction scratch exact site membership",
            ));
        }
        Ok(())
    }

    pub(crate) fn check_seal(&self, seal: &SiteSeal) -> StorageResult<()> {
        self.check(seal.scope(), SiteStage::FinalSealed)?;
        if self.seal.as_ref() != Some(seal) {
            return Err(StorageError::Integrity(
                "construction scratch exact site seal",
            ));
        }
        Ok(())
    }

    pub(crate) fn check_roots(&self, scope: &StateScope) -> StorageResult<()> {
        if scope != &self.roots || self.stage != SiteStage::Retired || self.failed.get() {
            return Err(StorageError::Integrity(
                "construction scratch sites not retired",
            ));
        }
        Ok(())
    }

    pub(crate) fn maximum(&self) -> Option<SiteKey> {
        self.membership.as_ref().and_then(SiteMembership::maximum)
    }

    pub(crate) fn description(&self) -> String {
        let mut description = format!(
            "site custody: scope={:?}, stage={:?}, records={}, remaining={}, maximum={:?}, after={:?}, birth={:?}, final={:?}",
            self.scope.as_bytes(), self.stage, self.records, self.remaining, self.maximum(), self.after,
            self.membership.as_ref().map(|members| members.birth().encode()), self.seal.as_ref().map(SiteSeal::encode),
        );
        if let Some(attempt) = &self.attempt {
            description.push_str(&format!(
                "; site attempt {:?}: before={}, proposed={}, records={:?}, proposed_flags={:?}, observations={:?}, prior_after={:?}, proposed_after={:?}",
                attempt.kind, attempt.before, attempt.proposed, &attempt.records[..attempt.count],
                &attempt.proposed_flags[..attempt.count], &attempt.observations[..attempt.observation_count], attempt.prior_after, attempt.proposed_after,
            ));
        }
        if let Some(expected) = &self.expected_birth {
            description.push_str(&format!("; expected birth={:?}", expected.encode()));
        }
        if let Some(proposed) = &self.proposed_membership {
            description.push_str(&format!("; proposed membership={:?}", proposed.encode()));
        }
        if let Some((seal, maximum)) = &self.proposed_seal {
            description.push_str(&format!(
                "; proposed final={:?}, maximum={maximum:?}",
                seal.encode()
            ));
        }
        description
    }
}
