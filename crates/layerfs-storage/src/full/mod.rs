//! Legacy Full-family persistence owners.

pub mod branch;
pub mod closure;
pub mod compaction;
pub mod layer_stack;
pub mod lease;
mod legacy_branch;
pub(crate) mod legacy_branch_transition;
pub(crate) mod legacy_layer_stack;
pub(crate) mod legacy_store;
pub mod operation;
pub mod receipt;
pub mod record_id;
pub mod store;
pub mod transfer;
