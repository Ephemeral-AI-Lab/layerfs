//! `store.pull`: bring a sqlite Store from its machine to native.
use layerfs_bridge::control::SdkValue;
use layerfs_control::calls::table::Kind::Name;
use layerfs_control::calls::table::{a, call, Call, PROGRESS};
use layerfs_control::calls::Context;
use layerfs_control::common::outcome::Outcome;

/// Its table entry.
pub(crate) const CALL: Call = call(
    "store",
    "pull",
    &[a("name", Name)],
    &[PROGRESS],
    "bring a sqlite Store from its machine to native",
    run,
);

/// Runs `store.pull`.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    layerfs_control::store::family::run(context, "pull", args)
}
