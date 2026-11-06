//! `backup.connect`: connect an r2 or MinIO bucket.
use crate::calls::table::Kind::{Name, Size, Text};
use crate::calls::table::{a, call, opt, Call};
use crate::calls::Context;
use layerfs_bridge::control::SdkValue;
use layerfs_home::outcome::Outcome;

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
    layerfs_store::backup::family::connect(&context.home, args)
}
