//! `backup.disconnect`: forget a bucket and this home's links to it.
use crate::calls::table::Kind::Name;
use crate::calls::table::{a, call, Call};
use crate::calls::Context;
use layerfs_bridge::control::SdkValue;
use layerfs_home::outcome::Outcome;

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
    layerfs_store::backup::family::disconnect(&context.home, args)
}
