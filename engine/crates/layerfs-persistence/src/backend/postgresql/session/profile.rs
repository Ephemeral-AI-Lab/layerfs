//! Per-transaction settings: schema, `synchronous_commit` and the lane wait.
//!
//! Every setting is applied with `SET LOCAL` inside the transaction it
//! governs, in the same round trip as `BEGIN`, so a transaction-pooling proxy
//! never carries one session's settings to another.
use crate::backend::records::BackendError;
use crate::{PostgresSelection, SynchronousCommit};

/// The checked settings of one PostgreSQL Store selection.
#[derive(Clone, Debug)]
pub(crate) struct Settings {
    pub(crate) schema: String,
    pub(crate) synchronous_commit: SynchronousCommit,
    pub(crate) lock_timeout_ms: u32,
}

impl Settings {
    pub(crate) fn new(selection: &PostgresSelection) -> Result<Self, BackendError> {
        let schema = &selection.schema;
        let valid = !schema.is_empty()
            && schema.len() <= 63
            && schema
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
            && !schema.as_bytes()[0].is_ascii_digit()
            && !schema.starts_with("pg_");
        if !valid {
            return Err(BackendError::Integrity);
        }
        Ok(Self {
            schema: schema.clone(),
            synchronous_commit: selection.synchronous_commit,
            lock_timeout_ms: selection.lock_timeout_ms,
        })
    }

    /// One read snapshot: every statement of the transaction sees one state.
    pub(crate) fn read_begin(&self) -> String {
        format!(
            "BEGIN ISOLATION LEVEL REPEATABLE READ READ ONLY; SET LOCAL search_path TO {}",
            self.schema
        )
    }

    /// A write: READ COMMITTED, never SERIALIZABLE. With `lane`, it first
    /// waits for the Store's lane row, so writes of every process run one at
    /// a time; a write nested in this thread's held lane already owns it.
    pub(crate) fn write_begin(&self, lane: bool) -> String {
        let mut sql = format!(
            "BEGIN ISOLATION LEVEL READ COMMITTED; SET LOCAL search_path TO {}; \
             SET LOCAL synchronous_commit TO {}",
            self.schema,
            self.synchronous_commit.setting()
        );
        if lane {
            sql.push_str(&self.lane_lock());
        }
        sql
    }

    /// The held publication lane: one open transaction owning the lane row.
    pub(crate) fn lane_begin(&self) -> String {
        format!(
            "BEGIN ISOLATION LEVEL READ COMMITTED; SET LOCAL search_path TO {}{}",
            self.schema,
            self.lane_lock()
        )
    }

    fn lane_lock(&self) -> String {
        format!(
            "; SET LOCAL lock_timeout TO {}; SELECT 1 FROM store_lane WHERE id = 1 FOR UPDATE",
            self.lock_timeout_ms
        )
    }
}
