//! `store.pull`: bring a sqlite Store from its machine to native.
use crate::calls::table::Kind::Name;
use crate::calls::table::{a, call, Call, PROGRESS};
use crate::calls::Context;
use layerfs_bridge::control::SdkValue;
use layerfs_home::outcome::Outcome;

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
    let (home, events) = context.parts();
    layerfs_control::store::pull(home, events, args)
}
