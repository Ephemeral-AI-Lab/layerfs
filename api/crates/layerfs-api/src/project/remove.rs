//! `project.remove`: remove one project from its Store.
use layerfs_bridge::control::SdkValue;
use layerfs_control::calls::table::Kind::Project;
use layerfs_control::calls::table::{a, call, Call};
use layerfs_control::calls::Context;
use layerfs_home::outcome::Outcome;

/// Its table entry.
pub(crate) const CALL: Call = call(
    "project",
    "remove",
    &[a("project", Project)],
    &[],
    "remove one project from its Store",
    run,
);

/// Runs `project.remove`.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    layerfs_control::project::family::run(context, "remove", args)
}
