//! `project.list`: every project in the home, or one Store's.
use layerfs_bridge::control::SdkValue;
use layerfs_control::calls::table::Kind::Name;
use layerfs_control::calls::table::{call, opt, Call, PAGE};
use layerfs_control::calls::Context;
use layerfs_control::common::outcome::Outcome;

/// Its table entry.
pub(crate) const CALL: Call = call(
    "project",
    "list",
    &[opt("store", Name)],
    &PAGE,
    "every project in the home, or one Store's",
    run,
);

/// Runs `project.list`.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    layerfs_control::project::family::run(context, "list", args)
}
