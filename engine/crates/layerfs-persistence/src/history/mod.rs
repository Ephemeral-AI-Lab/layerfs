//! Shared history operations over typed persistence transactions.
mod catalog;
pub(crate) mod counters;
pub(crate) mod graph;
pub(crate) mod index;
pub(crate) mod lineage;
mod provider;
pub(crate) mod statement;
pub use super::gc::mark::history::{KeptHistory, KeptNode, KeptNodeId};
pub use super::gc::mark::objects::GcTuple;
pub(crate) use super::gc::mark::objects::{live_heads_digest_in, mark_gc_reached, Reach};
pub use super::gc::mark::objects::{KeptObjects, KeptWaste, MarkResult};
pub(crate) use super::gc::spill::sort as spill;
pub use graph::fused::{Committer, Fused, FIXED_ROWS};
pub use index::hints::HINT_BATCH_ROWS;
pub(crate) use lineage::backfill::backfill_lineage;
pub use provider::HistoryProvider;
pub(crate) use statement::rows::{
    branch_row as diagnostic_branch, commit_graph_row as diagnostic_commit_graph,
    layer_row as diagnostic_layer, stack_row as diagnostic_stack,
};

pub use super::gc::mark::objects::{Sequence, Spans};
pub(crate) use graph::commit::insert_params as commit_insert_params;
use layerfs_history::query;
use statement::bindings::params;
pub(crate) use statement::bindings::{place_evidence, place_tokens};

pub(crate) use counters::facts::validate_stream as validate_facts_stream;
