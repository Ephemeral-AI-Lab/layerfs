//! Verified connection and transaction ownership.

mod open;
mod transaction;

#[cfg(test)]
pub(crate) use open::initial_verified_scrub;
pub(crate) use open::{
    add_retained_scrub_counters, add_verification_progress_counters, clear_known_trusted_history,
    inspect_store_id_readonly, mark_known_trusted_history, read_ref_reconcile_readonly,
    reopen_store_primary, trusted_history,
};
#[cfg(any(test, feature = "test-hooks"))]
pub(crate) use transaction::LostCommitAcknowledgementHook;
pub(crate) use transaction::{CommitDispatch, ConnectionGuard, SqliteCommit};
