//! `backup.disconnect`: forget a bucket and this home's links to it.
use layerfs_bridge::control::SdkValue;
use layerfs_control::calls::table::Kind::Name;
use layerfs_control::calls::table::{a, call, Call};
use layerfs_control::calls::Context;
use layerfs_control::common::outcome::Outcome;

/// Its table entry.
pub(crate) const CALL: Call = call(
    "backup",
    "disconnect",
    &[a("name", Name)],
    &[],
    "forget a bucket and this home's links to it",
    run,
);

/// Runs `backup.disconnect`.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    layerfs_control::backup::family::run(context, "disconnect", args)
}
