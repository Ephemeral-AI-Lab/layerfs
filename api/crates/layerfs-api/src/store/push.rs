//! `store.push`: send a sqlite Store from native to a machine.
use layerfs_bridge::control::SdkValue;
use layerfs_control::calls::table::Kind::Name;
use layerfs_control::calls::table::{a, call, Call, PROGRESS};
use layerfs_control::calls::Context;
use layerfs_home::outcome::Outcome;

/// Its table entry.
pub(crate) const CALL: Call = call(
    "store",
    "push",
    &[a("name", Name), a("machine", Name)],
    &[PROGRESS],
    "send a sqlite Store from native to a machine",
    run,
);

/// Runs `store.push`.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    layerfs_control::store::family::run(context, "push", args)
}
