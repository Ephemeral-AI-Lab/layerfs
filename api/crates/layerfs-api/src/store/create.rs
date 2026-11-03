//! `store.create`: an empty sqlite Store on native.
use layerfs_bridge::control::SdkValue;
use layerfs_control::calls::table::Kind::Name;
use layerfs_control::calls::table::{a, call, Call};
use layerfs_control::calls::Context;
use layerfs_control::common::outcome::Outcome;

/// Its table entry.
pub(crate) const CALL: Call = call(
    "store",
    "create",
    &[a("name", Name)],
    &[],
    "an empty sqlite Store on native",
    run,
);

/// Runs `store.create`.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    layerfs_control::store::family::run(context, "create", args)
}
