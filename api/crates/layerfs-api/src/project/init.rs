//! `project.init`: import a host folder as a new project into an existing Store.
use crate::calls::table::Kind::{Int, Project, Text};
use crate::calls::table::{a, call, opt, Call, PROGRESS};
use crate::calls::Context;
use layerfs_bridge::control::SdkValue;
use layerfs_home::names::ProjectRef;
use layerfs_home::outcome::Outcome;
use layerfs_store::project::family::{init, ProjectInit};

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
    let request = ProjectInit {
        project: ProjectRef::parse(args.get_text("project").unwrap_or(""))?,
        folder: args.get_text("folder").unwrap_or("").to_owned(),
        timeout: args.get_int("timeout"),
    };
    let (home, events) = context.parts();
    init(home, events, &request)
}
