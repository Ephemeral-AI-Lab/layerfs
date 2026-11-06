//! `store.status`: a Store's live state.
use layerfs_bridge::control::SdkValue;
use layerfs_control::calls::table::Kind::Name;
use layerfs_control::calls::table::{a, call, Call};
use layerfs_control::calls::Context;
use layerfs_home::outcome::Outcome;

/// Its table entry.
pub(crate) const CALL: Call = call(
    "store",
    "status",
    &[a("name", Name)],
    &[],
    "a Store's live state",
    run,
);

/// Runs `store.status`.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    layerfs_control::store::family::run(context, "status", args)
}
