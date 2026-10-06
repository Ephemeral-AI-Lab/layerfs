//! Typed outcome knowledge, independently of local slot or Branch publication.
use crate::runtime::RuntimeError;
use layerfs_storage::StorageError;

/// Exact knowledge supplied by an owning typed failure, without a resolver.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FailureKnowledge {
    /// Owning failure does not declare persistence uncertainty or failed cleanup.
    Known,
    /// Known failure with incomplete cleanup; original/cleanup errors stay distinct.
    RetainedFailure,
    /// An owning original or cleanup operation has an unknown persistence outcome.
    TerminalUnknown,
}
/// Classifies the owning error variants without reading/replaying a provider.
/// An unknown Save suboperation does not imply an unknown Branch transition.
/// CleanupFailed containing an unknown original/cleanup cause remains unknown;
/// a known cleanup failure remains separately retained rather than becoming unknown.
pub fn failure_knowledge(error: &RuntimeError) -> FailureKnowledge {
    match error {
        RuntimeError::Storage(error) => storage(error),
        RuntimeError::History(error) if error.unknown() => FailureKnowledge::TerminalUnknown,
        _ => FailureKnowledge::Known,
    }
}
fn storage(error: &StorageError) -> FailureKnowledge {
    if error.is_unknown_outcome() {
        return FailureKnowledge::TerminalUnknown;
    }
    match error {
        StorageError::CleanupFailed { original, cleanup } => {
            if storage(original) == FailureKnowledge::TerminalUnknown
                || storage(cleanup) == FailureKnowledge::TerminalUnknown
            {
                FailureKnowledge::TerminalUnknown
            } else {
                FailureKnowledge::RetainedFailure
            }
        }
        _ => FailureKnowledge::Known,
    }
}
