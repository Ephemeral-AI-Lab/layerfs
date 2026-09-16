//! Acknowledgement and the one failure boundary.
//!
//! Every failed operation crosses this boundary exactly once. A definite failure
//! marks the save terminal and cleans up established unpublished ownership a
//! single time. A failure whose persistence outcome is unproven quarantines the
//! save instead: nothing is resent, polled or deleted on a guess.

use crate::cas::owner::MutationOwner;
use crate::error::StorageError;

/// Applies the single terminal disposition for `error`.
pub fn terminate(owner: &mut MutationOwner, error: StorageError) -> StorageError {
    if error.is_unknown_outcome() {
        owner.quarantine();
        return error;
    }
    owner.mark_terminal();
    match owner.abandon() {
        Ok(()) => error,
        Err(cleanup) => StorageError::CleanupFailed {
            original: Box::new(error),
            cleanup: Box::new(cleanup),
        },
    }
}
