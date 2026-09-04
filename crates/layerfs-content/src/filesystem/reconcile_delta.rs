//! Caller-owned aggregate allowance for reconciliation's private final deltas.
use std::path::Path;

#[derive(Clone, Copy)]
pub struct ReconcileBudget<'a> {
    pub scratch_dir: &'a Path,
    pub memory_bytes: u64,
    pub spool_bytes: u64,
}

pub use super::reconcile_overlay::{
    replace_choices_from_snapshots_bounded, replace_paths_from_snapshot_bounded,
};
