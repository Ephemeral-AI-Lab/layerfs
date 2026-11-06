//! `project.init`: import a host folder as a new project into an existing Store.
use crate::calls::table::Kind::{Int, Project, Text};
use crate::calls::table::{a, call, opt, Call, PROGRESS};
use crate::calls::Context;
use layerfs_bridge::control::SdkValue;
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
    let (home, events) = context.parts();
    layerfs_store::project::family::init(home, events, args)
}
