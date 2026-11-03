//! `project.get`: a project's record: its Store, kind and machine.
use layerfs_bridge::control::SdkValue;
use layerfs_control::calls::table::Kind::Project;
use layerfs_control::calls::table::{a, call, Call};
use layerfs_control::calls::Context;
use layerfs_control::common::outcome::Outcome;

/// Its table entry.
pub(crate) const CALL: Call = call(
    "project",
    "get",
    &[a("project", Project)],
    &[],
    "a project's record: its Store, kind and machine",
    run,
);

/// Runs `project.get`.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    layerfs_control::project::family::run(context, "get", args)
}
