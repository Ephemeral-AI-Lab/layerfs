//! `backup.restore`: download a backup as a sqlite Store on native.
use layerfs_bridge::control::SdkValue;
use layerfs_control::calls::table::Kind::Name;
use layerfs_control::calls::table::{a, call, opt, Call, PROGRESS};
use layerfs_control::calls::Context;
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
    layerfs_control::backup::family::run(context, "restore", args)
}
