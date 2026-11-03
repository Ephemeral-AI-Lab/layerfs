//! `backup.list`: every Store file on the connected buckets.
use layerfs_bridge::control::SdkValue;
use layerfs_control::calls::table::Kind::Name;
use layerfs_control::calls::table::{call, opt, Call};
use layerfs_control::calls::Context;
use layerfs_control::common::outcome::Outcome;

/// Its table entry.
pub(crate) const CALL: Call = call(
    "backup",
    "list",
    &[opt("bucket", Name)],
    &[],
    "every Store file on the connected buckets",
    run,
);

/// Runs `backup.list`.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    layerfs_control::backup::family::run(context, "list", args)
}
