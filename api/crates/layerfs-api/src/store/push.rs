//! `store.push`: send a sqlite Store from native to a machine.
use crate::calls::table::Kind::Name;
use crate::calls::table::{a, call, Call, PROGRESS};
use crate::calls::Context;
use layerfs_bridge::control::SdkValue;
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
    let (home, events) = context.parts();
    layerfs_control::store::push(home, events, args)
}
