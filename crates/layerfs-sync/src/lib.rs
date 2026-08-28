//! Explicit bounded Fetch/Push data plane. Transfer alone changes no head.

#![forbid(unsafe_code)]

mod checkout;
pub mod endpoint;
mod history;
mod local;
mod object_transfer;
mod publication;
mod reconcile;
mod types;

pub use checkout::fetch_branch;
pub use endpoint::{DurableControlEndpoint, DurableEndpoint};
pub use local::LocalDurable;
pub use object_transfer::{abort_fetch_transfer, abort_push_transfer, fetch_objects, push_objects};
pub use publication::{
    push_branch, push_branch_rollback, push_child_branch_merge, push_layer_stack_genesis,
    push_layer_stack_merge, push_layer_stack_rollback,
};
pub use types::*;

pub const COMPONENT: &str = "layerfs-sync";
