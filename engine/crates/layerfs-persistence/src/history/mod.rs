//! Shared history operations over typed persistence transactions.
pub(crate) mod admission;
mod allocation;
mod ancestry;
mod bindings;
mod branch;
mod catalog;
mod commit;
mod digests;
mod epoch;
mod hints;
mod kept;
mod layerstack;
mod lineage;
mod lineage_backfill;
mod merge;
mod objects;
mod paths;
mod position;
mod provider;
mod removal;
mod rows;
mod transaction;
pub use kept::{KeptHistory, KeptNode, KeptNodeId};
pub(crate) use lineage_backfill::backfill_lineage;
pub(crate) use objects::spill;
pub use objects::GcTuple;
pub(crate) use objects::{mark_gc_reached, Reach};
pub use objects::{KeptObjects, KeptWaste, MarkResult};
pub use provider::HistoryProvider;
pub(crate) use rows::{
    branch_row as diagnostic_branch, commit_graph_row as diagnostic_commit_graph,
    layer_row as diagnostic_layer, stack_row as diagnostic_stack,
};

use bindings::params;
use layerfs_history::query;
pub(crate) mod facts;

pub(crate) use facts::validate_stream as validate_facts_stream;
