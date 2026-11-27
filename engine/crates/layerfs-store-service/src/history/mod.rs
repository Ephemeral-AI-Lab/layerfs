//! Branch history over the global Store: typed fork, merge, publish and
//! listings, and the lookups and refusals the Store calls share.
pub mod branch;
pub mod branch_remove;
pub mod gc_admission;
pub mod listing;
pub(crate) mod merge_units;
pub mod project_delete;
pub mod resolve;
pub mod rows;
pub mod saved;
pub mod whiteout;

pub use branch::MAIN;

#[doc(hidden)]
pub use crate::history::branch_remove as removal;
