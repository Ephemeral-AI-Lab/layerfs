//! `backup.create`: upload a sqlite Store to a bucket.
use layerfs_bridge::control::SdkValue;
use layerfs_control::calls::table::Kind::Name;
use layerfs_control::calls::table::{a, call, opt, Call, PROGRESS};
use layerfs_control::calls::Context;
use layerfs_control::common::outcome::Outcome;

/// Its table entry.
pub(crate) const CALL: Call = call(
    "backup",
    "create",
    &[a("store", Name), a("bucket", Name), opt("file", Name)],
    &[PROGRESS],
    "upload a sqlite Store to a bucket",
    run,
);

/// Runs `backup.create`.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    layerfs_control::backup::family::run(context, "create", args)
}
