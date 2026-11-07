//! `project.export`: export a project's snapshot from its Store to a local folder.
use crate::calls::table::Kind::{Name, Project, Text};
use crate::calls::table::{a, call, opt, Call, PROGRESS};
use crate::calls::Context;
use layerfs_bridge::control::SdkValue;
use layerfs_home::names::ProjectRef;
use layerfs_home::outcome::Outcome;
use layerfs_store::project::export::{run as export, ProjectExport};

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
    let request = ProjectExport {
        project: ProjectRef::parse(args.get_text("project").unwrap_or(""))?,
        destination: args.get_text("destination").unwrap_or("").to_owned(),
        branch: args.get_text("branch").map(str::to_owned),
    };
    let (home, events) = context.parts();
    export(home, events, &request)
}
