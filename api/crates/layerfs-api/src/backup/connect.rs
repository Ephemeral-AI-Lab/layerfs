//! `backup.connect`: connect an r2 or MinIO bucket.
use layerfs_bridge::control::SdkValue;
use layerfs_control::calls::table::Kind::{Name, Size, Text};
use layerfs_control::calls::table::{a, call, opt, Call};
use layerfs_control::calls::Context;
use layerfs_control::common::outcome::Outcome;

/// Its table entry.
pub(crate) const CALL: Call = call(
    "backup",
    "connect",
    &[a("name", Name), a("url", Text)],
    &[
        opt("key", Text),
        opt("region", Text),
        opt("part_size", Size),
    ],
    "connect an r2 or MinIO bucket",
    run,
);

/// Runs `backup.connect`.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    layerfs_control::backup::family::run(context, "connect", args)
}
