//! `store.list`: every Store, from the home's records.
use layerfs_bridge::control::SdkValue;
use layerfs_control::calls::table::{call, Call};
use layerfs_control::calls::Context;
use layerfs_control::common::outcome::Outcome;

/// Its table entry.
pub(crate) const CALL: Call = call(
    "store",
    "list",
    &[],
    &[],
    "every Store, from the home's records",
    run,
);

/// Runs `store.list`.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    layerfs_control::store::family::run(context, "list", args)
}
