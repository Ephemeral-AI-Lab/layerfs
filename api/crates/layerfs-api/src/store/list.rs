//! `store.list`: every Store, from the home's records.
use crate::calls::table::{call, Call};
use crate::calls::Context;
use layerfs_bridge::control::SdkValue;
use layerfs_home::outcome::Outcome;

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
fn run(context: &mut Context<'_>, _args: &SdkValue) -> Outcome {
    layerfs_store::sqlite::family::list(&context.home)
}
