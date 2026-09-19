//! `history.*`: the retained-history storage claim — three rows, one per selection.
//!
//! **This group is outside the 217.** It is not in `families::ALL`, not in
//! `FROZEN_CARDINALITY`, not in `registry::cases()`, not in `--smoke` and not in
//! `--lane full`. It carries its own cardinality of three, its own lanes, its own
//! golden table and its own verification default, and it amends no 217-row verdict.
//! `registry::history_cases` is the only accessor that sees it.
//!
//! Rows only. The operation, the corpus reading and the oracle bodies live in
//! `ops/history.rs` and `workload/history.rs`, exactly as `families/mod.rs`
//! requires of every other family.
//!
//! **No row is in `--smoke`,** and that is deliberate rather than an oversight:
//! `--smoke` is one tier per family and these rows are whole repository histories.
//! `benchmark_rules.md` §15 forbids a default invocation launching an endurance
//! run, so `history-stride1` is explicitly selectable and is never a default.

use super::CaseSpec;
use crate::registry::{CacheState, Case, Preparation, Shape, StoreState};
use crate::workload::history::Row;

/// Group identifier.
pub const GROUP: &str = "history.*";

/// The three rows, in tier order.
///
/// `bytes` is the selection's cumulative logical bytes and `entries` its state
/// count, so the byte axis and the state axis are both carried by the existing
/// `Case` fields rather than by a second table.
pub fn cases() -> Vec<Case> {
    Row::ALL
        .iter()
        .map(|row| {
            CaseSpec::new(row.id(), GROUP, Shape::History(*row))
                .byte_tier(0, row.logical_bytes(), "")
                .entry_tier(0, row.states() as u32, "")
                .cache(CacheState::CreatedInSample)
                .store(StoreState::CreatedInSample)
                .prepared(Preparation::InProcess)
                .fixed()
                .build()
        })
        .collect()
}
