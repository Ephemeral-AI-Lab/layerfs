//! `backup.create`: upload a sqlite Store to a bucket.
use crate::calls::table::Kind::Name;
use crate::calls::table::{a, call, opt, Call, PROGRESS};
use crate::calls::Context;
use layerfs_bridge::control::SdkValue;
use layerfs_home::outcome::Outcome;

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
    let (home, events) = context.parts();
    layerfs_store::backup::family::create(home, events, args)
}
