//! `store.remove`: delete a sqlite Store with every project in it.
use layerfs_bridge::control::SdkValue;
use layerfs_control::calls::table::Kind::Name;
use layerfs_control::calls::table::{a, call, Call};
use layerfs_control::calls::Context;
use layerfs_control::common::outcome::Outcome;

/// Its table entry.
pub(crate) const CALL: Call = call(
    "store",
    "remove",
    &[a("name", Name)],
    &[],
    "delete a sqlite Store with every project in it",
    run,
);

/// Runs `store.remove`.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    layerfs_control::store::family::run(context, "remove", args)
}
