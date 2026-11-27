//! The compaction phase lists and the `store.status` compaction projection.
//!
//! The §11 report itself is [`crate::CompactReceipt`]: both backends return
//! that one type and the control layer renders it with one serializer. §11
//! names are logical; the rendered keys are snake_case (§10).

/// The phases a SQLite compaction passes, in order (§11): a run that crashes
/// leaves its record at the phase it reached.
pub const SQLITE_COMPACT_PHASES: [&str; 5] = ["mark", "build", "verify", "install", "cleanup"];

/// The PostgreSQL §8 compaction phases, in order: Phase M mark, Phase A
/// build, Phase B swap and Phase C cleanup. These are the values a
/// PostgreSQL `gc_op` row records, distinct from the SQLite §7 phases.
pub const POSTGRES_COMPACT_PHASES: [&str; 4] = ["mark", "build", "swap", "cleanup"];

/// The operation `store.status` shows while a compaction runs, or after a dead
/// owner left one (§11): `mark`, `build`, `verify` or `install`.
///
/// It is read from `<store>.compact-op` alone; the Store database, its
/// artifacts and the directory listing are never opened for it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompactStatus {
    /// The operation identity as `op-<hex>`.
    pub id: String,
    /// The call that owns it: `store.compact`.
    pub name: &'static str,
    /// The phase the run recorded.
    pub phase: String,
    /// When the run started, in Unix milliseconds.
    pub started: i64,
    /// Whether the outcome is unknown: a settled run removes its record.
    pub uncertain: bool,
    /// `busy` while the owner lives, `unresolved` once it is gone.
    pub state: &'static str,
    /// The backend's phases, in order, for a progress view.
    pub phases: &'static [&'static str],
}
