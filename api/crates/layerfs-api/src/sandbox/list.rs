//! `sandbox.list`: every Sandbox in the home, or one project's.
use crate::calls::table::Kind::Project;
use crate::calls::table::{call, opt, Call, PAGE};
use crate::calls::Context;
use layerfs_bridge::control::SdkValue;
use layerfs_home::outcome::Outcome;

/// Its table entry.
pub(crate) const CALL: Call = call(
    "sandbox",
    "list",
    &[opt("project", Project)],
    &PAGE,
    "every Sandbox in the home, or one project's",
    run,
);

/// Runs `sandbox.list`.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    layerfs_sandbox::family::list(&context.home, args)
}
