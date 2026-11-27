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

// #828 compat: old public module paths, removed in wave 3.
#[doc(hidden)]
pub use crate::history::branch_remove as removal;
#[doc(hidden)]
pub use crate::history::gc_admission as admission; // #828 compat
#[doc(hidden)]
pub use crate::history::project_delete as deletion; // #828 compat
#[doc(hidden)]
pub use crate::lookup; // #828 compat // #828 compat
