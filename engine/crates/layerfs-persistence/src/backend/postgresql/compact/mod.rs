//! §8 PostgreSQL Store compaction: the mark, the scratch rebuild, the pack swap
//! and the cleanup.
//!
//! A `store.compact` registers its admission intent in the `gc_op` row under
//! the Store's writer lane, runs Phase M (mark the reached set and persist its
//! tuple) and then either stops on the §3 `none` path or runs Phase A (build the
//! retained payload in a scratch schema), Phase B (swap `pack` and
//! `object_location` at `G → G+1` and verify the new tuple) and Phase C
//! (delete only the persisted dead set and settle the totals in one
//! transaction). §9's leftover rule is judged by the generation the `gc_op`
//! row records.
//!
//! The module is split by phase: [`types`] (the §8 shapes), [`op`] (the mark
//! and the op row), [`build`] (Phase A), [`swap`] (Phase B), [`cleanup`]
//! (Phase C and leftover settling), [`sync`] (the §8 step-10 predicate),
//! [`report`] (§11) and [`run`] (the orchestration on `PostgresHandles`).

mod build;
mod cleanup;
mod op;
mod report;
mod run;
mod swap;
mod sync;
mod types;

pub use report::{decode_kept_waste, encode_kept_waste, report};
pub use sync::{effective_rank, standby_satisfied, sync_name, sync_rank, Replica};
