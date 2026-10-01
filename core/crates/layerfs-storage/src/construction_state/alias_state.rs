//! Captured same-file frontier class and heap-owned bounded failure capsule.
use crate::{StorageError, StorageResult};
use layerfs_content::filesystem::state::{
    AliasCapacity, AliasCurrent, AliasFact, AliasProgress, AliasSeal, GraphMemory,
    GraphMemoryLease, SiteMembership, SiteScope,
};
use std::cell::Cell;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct AliasTotals {
    pub(crate) sequence: u64,
    pub(crate) facts: u64,
    pub(crate) jobs: u64,
    pub(crate) expanded: u64,
    pub(crate) remaining: u64,
    pub(crate) after: Option<u64>,
}
pub(crate) struct AliasAttemptData {
    pub(crate) kind: &'static str,
    pub(crate) before: AliasTotals,
    pub(crate) after: AliasTotals,
    pub(crate) old_current: Option<AliasCurrent>,
    pub(crate) new_current: Option<AliasCurrent>,
    pub(crate) old_progress: AliasProgress,
    pub(crate) new_progress: AliasProgress,
    pub(crate) old_stage: u8,
    pub(crate) new_stage: u8,
    pub(crate) changes: Vec<(Option<AliasFact>, Option<AliasFact>)>,
}
impl AliasAttemptData {
    pub(crate) fn new(state: &Aliases, kind: &'static str) -> StorageResult<Self> {
        let mut changes = Vec::new();
        changes.try_reserve_exact(128).map_err(|_| {
            StorageError::Content(layerfs_content::ContentError::ResourceUnavailable {
                what: "alias.attempt",
            })
        })?;
        if changes.capacity() > 128 {
            return Err(StorageError::Integrity("alias attempt allocation capacity"));
        }
        Ok(Self {
            kind,
            before: state.totals,
            after: state.totals,
            old_current: state.current,
            new_current: state.current,
            old_progress: state.progress.clone(),
            new_progress: state.progress.clone(),
            old_stage: state.stage,
            new_stage: state.stage,
            changes,
        })
    }
}
pub(crate) struct Aliases {
    pub(crate) scope: SiteScope,
    pub(crate) capacity: AliasCapacity,
    pub(crate) members: Option<SiteMembership>,
    pub(crate) totals: AliasTotals,
    pub(crate) current: Option<AliasCurrent>,
    pub(crate) progress: AliasProgress,
    pub(crate) stage: u8,
    pub(crate) failed: Cell<bool>,
    pub(crate) attempt: Option<AliasAttempt>,
    pub(crate) memory: GraphMemory,
    pub(crate) seal: Option<AliasSeal>,
}
impl Aliases {
    // Construction returns the funded owner so its allocation and lease stay inseparable.
    #[allow(clippy::new_ret_no_self)]
    pub(crate) fn new(
        scope: SiteScope,
        capacity: AliasCapacity,
        memory: GraphMemory,
    ) -> StorageResult<AliasesOwner> {
        let lease =
            memory.reserve(std::mem::size_of::<Self>() + std::mem::size_of::<AliasesOwner>())?;
        Ok(AliasesOwner {
            value: Box::new(Self {
                memory,
                scope,
                capacity,
                members: None,
                totals: AliasTotals::default(),
                current: None,
                progress: AliasProgress::initial(),
                stage: 0,
                failed: Cell::new(false),
                attempt: None,
                seal: None,
            }),
            _memory: lease,
        })
    }
    pub(crate) fn check(&self, members: &SiteMembership) -> StorageResult<()> {
        if members.birth().scope() != &self.scope
            || self.members.as_ref().is_some_and(|m| m != members)
        {
            return Err(StorageError::Content(
                layerfs_content::ContentError::InvalidOrderingRecord("alias foreign membership"),
            ));
        }
        if self.failed.get() || self.stage == 4 {
            return Err(StorageError::Integrity("alias terminal owner"));
        }
        Ok(())
    }
    pub(crate) fn acknowledge(&mut self) {
        let AliasAttempt { value, _memory } = self.attempt.take().unwrap();
        let AliasAttemptData {
            after,
            new_current,
            new_progress,
            new_stage,
            changes,
            ..
        } = *value;
        self.totals = after;
        self.current = new_current;
        self.progress = new_progress;
        self.stage = new_stage;
        drop(changes);
        drop(_memory);
    }
    pub(crate) fn description(&self) -> String {
        let mut result = format!(
            "alias custody: scope={:?}, stage={}, totals={:?}, current={:?}, progress={:?}",
            self.scope.as_bytes(),
            self.stage,
            self.totals,
            self.current,
            self.progress
        );
        if let Some(a) = &self.attempt {
            result.push_str(&format!("; alias attempt {}: totals={:?}->{:?}, current={:?}->{:?}, progress={:?}->{:?}, stage={}->{}, changes={:?}",a.kind,a.before,a.after,a.old_current,a.new_current,a.old_progress,a.new_progress,a.old_stage,a.new_stage,a.changes));
        }
        result
    }
}

/// Payload is freed before its last-owner credit.
pub(crate) struct AliasesOwner {
    value: Box<Aliases>,
    _memory: GraphMemoryLease,
}
impl std::ops::Deref for AliasesOwner {
    type Target = Aliases;
    fn deref(&self) -> &Aliases {
        &self.value
    }
}
impl std::ops::DerefMut for AliasesOwner {
    fn deref_mut(&mut self) -> &mut Aliases {
        &mut self.value
    }
}
/// Bounded Vec and boxed attempt are freed before their last-owner credit.
pub(crate) struct AliasAttempt {
    value: Box<AliasAttemptData>,
    _memory: GraphMemoryLease,
}
impl AliasAttempt {
    pub(crate) fn new(state: &Aliases, kind: &'static str) -> StorageResult<Self> {
        let bytes = std::mem::size_of::<AliasAttemptData>()
            + std::mem::size_of::<Self>()
            + 128 * std::mem::size_of::<(Option<AliasFact>, Option<AliasFact>)>();
        let lease = state.memory.reserve(bytes)?;
        Ok(Self {
            value: Box::new(AliasAttemptData::new(state, kind)?),
            _memory: lease,
        })
    }
}
impl std::ops::Deref for AliasAttempt {
    type Target = AliasAttemptData;
    fn deref(&self) -> &AliasAttemptData {
        &self.value
    }
}
impl std::ops::DerefMut for AliasAttempt {
    fn deref_mut(&mut self) -> &mut AliasAttemptData {
        &mut self.value
    }
}
