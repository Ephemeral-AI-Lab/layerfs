//! `project.export`: export a project's snapshot from its Store to a local folder.
use layerfs_bridge::control::SdkValue;
use layerfs_control::calls::table::Kind::{Name, Project, Text};
use layerfs_control::calls::table::{a, call, opt, Call, PROGRESS};
use layerfs_control::calls::Context;
use layerfs_control::common::outcome::Outcome;

/// Its table entry.
pub(crate) const CALL: Call = call(
    "project",
    "export",
    &[a("project", Project), a("destination", Text)],
    &[opt("branch", Name), PROGRESS],
    "export a project's snapshot from its Store to a local folder",
    run,
);

/// Runs `project.export`.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    layerfs_control::project::family::run(context, "export", args)
}
