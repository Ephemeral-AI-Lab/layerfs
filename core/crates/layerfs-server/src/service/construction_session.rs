//! Explicit native versus verified-zero operation custody on the same coordinator.
use layerfs_content::filesystem::state::{GraphConstructionScopes, VerifiedEmptyState};
use layerfs_storage::{construction_state::ScratchSession, StorageError, StorageResult};

// Captured Small authority remains inline; boxing would add an unadmitted owner.
#[allow(clippy::large_enum_variant)]
pub(crate) enum ConstructionSession {
    Native(ScratchSession),
    Empty(VerifiedEmptyState),
    Small(Option<layerfs_content::filesystem::rows::PendingSmallFiles>),
}
impl ConstructionSession {
    pub(crate) fn scopes(&self) -> StorageResult<Option<GraphConstructionScopes>> {
        match self {
            Self::Empty(state) => Ok(Some(state.scopes().clone())),
            Self::Small(_) => Ok(None),
            Self::Native(state) => {
                let subject = state.graph_subject().cloned().ok_or(
                    layerfs_content::ContentError::InvalidOrderingRecord("namespace subject"),
                )?;
                Ok(Some(GraphConstructionScopes::new(
                    state.selection().clone(),
                    subject,
                )?))
            }
        }
    }
    pub(crate) fn take_failure(&mut self) -> Option<StorageError> {
        match self {
            Self::Native(state) => state.take_failure(),
            Self::Empty(_) | Self::Small(_) => None,
        }
    }
    pub(crate) fn is_quarantined(&self) -> bool {
        match self {
            Self::Native(state) => state.is_quarantined(),
            Self::Empty(_) | Self::Small(_) => false,
        }
    }
    pub(crate) fn return_to_idle(&mut self) -> StorageResult<()> {
        match self {
            Self::Native(state) => state.return_to_idle(),
            Self::Empty(_) | Self::Small(_) => self.release(),
        }
    }
    pub(crate) fn release(&mut self) -> StorageResult<()> {
        match self {
            Self::Native(state) => state.release(),
            Self::Small(pending) => {
                drop(pending.take());
                Ok(())
            }
            Self::Empty(state) => {
                // Pure own logical abandonment/close, with no native cleanup or refund.
                state.abandon();
                Ok(())
            }
        }
    }
}
