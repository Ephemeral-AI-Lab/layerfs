//! `project.delete`: retire one project in its Store and remove its home entry.
use crate::calls::table::Kind::Project;
use crate::calls::table::{a, call, Call};
use crate::calls::Context;
use layerfs_bridge::control::SdkValue;
use layerfs_home::names::ProjectRef;
use layerfs_home::outcome::Outcome;

/// Its table entry.
pub(crate) const CALL: Call = call(
    "project",
    "delete",
    &[a("project", Project)],
    &[],
    "retire one project in its Store and remove its home entry",
    run,
);

/// Runs `project.delete`.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let project = ProjectRef::parse(args.get_text("project").unwrap_or(""))?;
    let (home, events) = context.parts();
    layerfs_control::project::delete(home, events, &project)
}
