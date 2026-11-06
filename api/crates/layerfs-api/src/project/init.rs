//! `project.init`: import a host folder as a new project into an existing Store.
use layerfs_bridge::control::SdkValue;
use layerfs_control::calls::table::Kind::{Int, Project, Text};
use layerfs_control::calls::table::{a, call, opt, Call, PROGRESS};
use layerfs_control::calls::Context;
use layerfs_home::outcome::Outcome;

/// Its table entry.
pub(crate) const CALL: Call = call(
    "project",
    "init",
    &[a("project", Project), a("folder", Text)],
    &[opt("timeout", Int), PROGRESS],
    "import a host folder as a new project into an existing Store",
    run,
);

/// Runs `project.init`.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    layerfs_control::project::family::run(context, "init", args)
}
