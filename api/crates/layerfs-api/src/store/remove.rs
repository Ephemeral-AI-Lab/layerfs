//! `store.remove`: delete a sqlite Store with every project in it.
use crate::calls::table::Kind::Name;
use crate::calls::table::{a, call, Call};
use crate::calls::Context;
use layerfs_bridge::control::SdkValue;
use layerfs_home::outcome::Outcome;

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
    layerfs_control::store::remove(&context.home, args)
}
