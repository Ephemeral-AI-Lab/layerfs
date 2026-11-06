//! `backup.restore`: download a backup as a sqlite Store on native.
use crate::calls::table::Kind::Name;
use crate::calls::table::{a, call, opt, Call, PROGRESS};
use crate::calls::Context;
use layerfs_bridge::control::SdkValue;
use layerfs_home::outcome::Outcome;

/// Its table entry.
pub(crate) const CALL: Call = call(
    "backup",
    "restore",
    &[a("store", Name), a("bucket", Name), opt("file", Name)],
    &[PROGRESS],
    "download a backup as a sqlite Store on native",
    run,
);

/// Runs `backup.restore`.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let (home, events) = context.parts();
    layerfs_control::backup::restore(home, events, args)
}
