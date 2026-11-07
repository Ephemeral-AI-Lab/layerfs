//! `backup.list`: every Store file on the connected buckets.
use crate::calls::table::Kind::Name;
use crate::calls::table::{call, opt, Call};
use crate::calls::Context;
use layerfs_bridge::control::SdkValue;
use layerfs_home::outcome::Outcome;

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
    layerfs_store::backup::family::list(&context.home, args.get_text("bucket"))
}
