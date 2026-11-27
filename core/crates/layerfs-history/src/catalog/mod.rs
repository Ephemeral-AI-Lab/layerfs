//! The history catalog: its provider contract, path grammar and bounded queries.
//!
//! Entry module: declarations and re-exports only.

pub mod contract;
pub mod path;
pub mod query;

pub use contract::{
    first_parent_window_serial, read_context_serial, ChainRow, ContextTarget, HistoryCatalog,
    HistoryCatalogConfig, MountMarkers, NamedSnapshot, NodeRecord, NodeRemovals, PublishLane,
    ReadContext,
};
