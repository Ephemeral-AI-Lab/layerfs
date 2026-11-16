//! §8 compaction types and constants: the dead totals, the op row and the built/swapped run.

/// The §3 rebuild threshold: rebuild when dead * 10 >= stored.
pub(crate) const THRESHOLD: u64 = 10;

/// `lfs_gc_<hex>`: the deterministic scratch schema name.
pub(crate) const SCRATCH_PREFIX: &str = "lfs_gc_";

/// §3: rebuild when the dead bytes the mark measured are at least a tenth of
/// the stored bytes the same mark read. Both totals come from the mark
/// snapshot: a live `pack` total read after the mark is not its denominator,
/// because a publication in between would flip a `none` decision to `rebuild`.
pub fn takes_rebuild_path(stored_at_mark: i64, live_stored: i64) -> bool {
    let stored = u64::try_from(stored_at_mark).unwrap_or(0);
    let live = u64::try_from(live_stored).unwrap_or(0);
    stored > 0 && stored.saturating_sub(live).saturating_mul(THRESHOLD) >= stored
}

/// The three wholly dead domains the swap records before COMMIT.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct DeadTotals {
    pub(crate) canonical: i64,
    pub(crate) stored: i64,
    pub(crate) metadata: i64,
    pub(crate) logical: i64,
}

/// One compaction op row, as the next call judges it.
#[derive(Clone, Debug)]
pub(crate) struct OpRow {
    pub(crate) generation: i64,
    pub(crate) phase: String,
    pub(crate) artifact: Option<String>,
    pub(crate) dead: DeadTotals,
    pub(crate) totals_applied: bool,
    pub(crate) pid: Option<i64>,
    pub(crate) backend_start: Option<String>,
    /// `pg_total_relation_size(pack,object_location)` before the swap, so a
    /// resumed `G + 1` report can still answer `physicalShrink`.
    pub(crate) physical_before: u64,
    /// The scratch schema's extent at the end of Phase A, the run's peak temp.
    pub(crate) peak_temp: u64,
    /// The mark's kept-waste accounting, for a resumed `G + 1` report.
    pub(crate) kept_waste: crate::backend::sqlite::compact::CompactKeptWaste,
    /// `store_totals.stored` at the mark, for the resumed report's zero reasons.
    pub(crate) total_stored: i64,
}

/// What Phase A leaves for Phase B.
pub(crate) struct Built {
    pub(crate) scratch: String,
    pub(crate) scratch_bytes: u64,
    pub(crate) dead: DeadTotals,
}

/// What the swap records for the report and Phase C.
pub(crate) struct Swapped {
    pub(crate) physical_before: u64,
    pub(crate) physical_after: u64,
    pub(crate) peak_temp: u64,
    pub(crate) dead: DeadTotals,
}
