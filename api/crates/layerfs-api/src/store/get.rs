//! `store.get`: a Store's record.
use layerfs_bridge::control::SdkValue;
use layerfs_control::calls::table::Kind::Name;
use layerfs_control::calls::table::{a, call, Call};
use layerfs_control::calls::Context;
use layerfs_control::common::outcome::Outcome;

/// Its table entry.
pub(crate) const CALL: Call = call(
    "store",
    "get",
    &[a("name", Name)],
    &[],
    "a Store's record",
    run,
);

/// Runs `store.get`.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    layerfs_control::store::family::run(context, "get", args)
}
