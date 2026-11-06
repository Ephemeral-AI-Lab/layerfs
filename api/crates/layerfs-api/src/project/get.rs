//! `project.get`: a project's record: its Store, kind and machine.
use crate::calls::table::Kind::Project;
use crate::calls::table::{a, call, Call};
use crate::calls::Context;
use layerfs_bridge::control::SdkValue;
use layerfs_home::outcome::Outcome;

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
    layerfs_store::project::family::get(&context.home, args)
}
