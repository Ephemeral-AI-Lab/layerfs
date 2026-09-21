//! Explicit staged and composite Commit outcomes and local installation.
use crate::{backing::metadata::MetadataCharge, StageSelector, WorkspaceError};
use layerfs_bridge::contract::{CommitOutcomeWire, Root};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommitReport {
    pub generation: u64,
    pub stage_token: Option<u64>,
    pub outcome: CommitOutcomeWire,
    pub revision: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommitPhase {
    Preparing,
    CommitStaged,
    CompositeCommit,
    Reconcile,
    Cleanup,
    Complete,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommitFailureDisposition {
    KnownBeforeCommit,
    Unknown,
    KnownCommitLocalFailure,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CommitStatus {
    pub phase: CommitPhase,
    pub known_root: Option<Root>,
    pub known_head: Option<[u8; 33]>,
    pub installed_revision: Option<u64>,
    pub failure: Option<CommitFailureDisposition>,
}
pub struct CommitFailure {
    pub generation: u64,
    pub phase: CommitPhase,
    pub disposition: CommitFailureDisposition,
    pub cause: WorkspaceError,
    pub stage: Option<StageSelector>,
    pub observed_stage: Option<layerfs_bridge::contract::StageWire>,
    pub known_outcome: Option<CommitOutcomeWire>,
    pub observed_outcome: Option<CommitOutcomeWire>,
    pub installed_revision: Option<u64>,
    pub(crate) _charge: MetadataCharge,
}
impl std::fmt::Debug for CommitFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CommitFailure")
            .field("generation", &self.generation)
            .field("phase", &self.phase)
            .field("disposition", &self.disposition)
            .field("cause", &self.cause)
            .field("stage", &self.stage)
            .field("observed_stage", &self.observed_stage)
            .field("known_outcome", &self.known_outcome)
            .field("observed_outcome", &self.observed_outcome)
            .field("installed_revision", &self.installed_revision)
            .finish()
    }
}
impl PartialEq for CommitFailure {
    fn eq(&self, other: &Self) -> bool {
        self.generation == other.generation
            && self.phase == other.phase
            && self.disposition == other.disposition
            && self.cause == other.cause
            && self.stage.as_ref().map(StageSelector::stage)
                == other.stage.as_ref().map(StageSelector::stage)
            && self.observed_stage == other.observed_stage
            && self.known_outcome == other.known_outcome
            && self.observed_outcome == other.observed_outcome
            && self.installed_revision == other.installed_revision
    }
}
impl Eq for CommitFailure {}
