//! Full authenticated traversal for frozen legacy_full evaluator materialization.

mod api;
mod output;
mod traversal;

pub(crate) use api::materialize_workspace;
pub(crate) use output::metadata;
