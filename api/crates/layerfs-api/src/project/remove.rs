//! `project.remove`: remove one project from its Store.
use crate::calls::table::Kind::Project;
use crate::calls::table::{a, call, Call};
use crate::calls::Context;
use layerfs_bridge::control::SdkValue;
use layerfs_home::names::ProjectRef;
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
    let project = ProjectRef::parse(args.get_text("project").unwrap_or(""))?;
    let (home, events) = context.parts();
    layerfs_control::project::remove(home, events, &project)
}
