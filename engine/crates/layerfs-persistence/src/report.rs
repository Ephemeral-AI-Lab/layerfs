//! The §11 compaction report and the `store.status` compaction projection.
//!
//! One shape serves every backend. A SQLite [`CompactReceipt`] projects into
//! [`CompactReport`] through [`CompactReportSource`], and T9's PostgreSQL
//! rebuild implements that trait for its own receipt, so both paths share one
//! wire shape and one serializer. §11 names are logical; the rendered keys are
//! snake_case (§10).
use crate::backend::sqlite::compact::{
    CompactKeptWaste, CompactPath, CompactReceipt, CompactResidue, CompactZeroReason,
};

/// The phases a SQLite compaction passes, in order (§11): a run that crashes
/// leaves its record at the phase it reached.
pub const SQLITE_COMPACT_PHASES: [&str; 5] = ["mark", "build", "verify", "install", "cleanup"];

/// The PostgreSQL §8 compaction phases, in order: Phase M mark, Phase A
/// build, Phase B swap and Phase C cleanup. These are the values a
/// PostgreSQL `gc_op` row records, distinct from the SQLite §7 phases.
pub const POSTGRES_COMPACT_PHASES: [&str; 4] = ["mark", "build", "swap", "cleanup"];

/// The generation a compaction started at and, when it installed a rebuild,
/// the one it left (`none` keeps both equal).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CompactGeneration {
    /// The generation the mark ran at.
    pub before: u64,
    /// The installed generation; equal to `before` on the `none` path.
    pub after: u64,
}

/// The §11 report `store.compact` returns, backend-neutral.
///
/// SQLite fills it from [`CompactReceipt`]; T9's PostgreSQL rebuild fills the
/// same fields from its own receipt. A backend-only field is deliberately not
/// defined here: PostgreSQL reports its own zeros through the same keys.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompactReport {
    /// `none` or `rebuild`.
    pub path: CompactPath,
    /// The generation before and after.
    pub generation: CompactGeneration,
    /// Bytes in wholly dead packs after the metadata walk.
    pub dead_bytes_bound: u64,
    /// The same bytes after the payload sweep (§7 step 4). No second sweep
    /// runs, so this equals `dead_bytes_bound` on every path (#42 A10).
    pub dead_bytes_refined: u64,
    /// The canonical bytes of the collected objects, deduplicated by object
    /// id: not the history rows' `logical` sum (#42 A8).
    pub logical_discarded: u64,
    /// The source Store's pre-mark free pages × page size on a rebuild; the
    /// live Store's count on `none` (#42 A8).
    pub backend_reusable: u64,
    /// Old file size minus the new one.
    pub physical_shrink: u64,
    /// Bytes the mark keeps that no retained root needs.
    pub kept_waste: CompactKeptWaste,
    /// Peak temporary bytes the run needed.
    pub peak_temp_bytes: u64,
    /// The artifact set left after settle; empty on success.
    pub residue: CompactResidue,
    /// §13: the report is an estimate, not a measurement.
    pub estimate: bool,
    /// One reason per zero report field, from the closed vocabulary.
    pub zero_reasons: Vec<CompactZeroReason>,
}

/// A backend receipt that projects to the one §11 [`CompactReport`].
///
/// SQLite's [`CompactReceipt`] implements it here. T9's PostgreSQL rebuild
/// implements it for the receipt of its Phase M/A/B/C run, which keeps the
/// PostgreSQL path on this shared shape without a PostgreSQL-only field.
pub trait CompactReportSource {
    /// Projects this receipt as the §11 report.
    fn compact_report(&self) -> CompactReport;
}

impl CompactReportSource for CompactReceipt {
    fn compact_report(&self) -> CompactReport {
        CompactReport {
            path: self.path,
            generation: CompactGeneration {
                before: self.generation_before,
                after: self.generation_after,
            },
            dead_bytes_bound: self.dead_bytes_bound,
            dead_bytes_refined: self.dead_bytes_refined,
            logical_discarded: self.logical_discarded,
            backend_reusable: self.backend_reusable,
            physical_shrink: self.physical_shrink,
            kept_waste: self.kept_waste.clone(),
            peak_temp_bytes: self.peak_temp_bytes,
            residue: self.residue.clone(),
            estimate: self.estimate,
            zero_reasons: self.zero_reasons.clone(),
        }
    }
}

impl From<&CompactReceipt> for CompactReport {
    fn from(receipt: &CompactReceipt) -> Self {
        receipt.compact_report()
    }
}

impl From<CompactReceipt> for CompactReport {
    fn from(receipt: CompactReceipt) -> Self {
        receipt.compact_report()
    }
}

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
