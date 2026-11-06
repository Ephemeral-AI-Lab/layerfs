//! `branch.list`: a project's Branches.
use crate::calls::table::Kind::Project;
use crate::calls::table::{a, call, Call, PAGE};
use crate::calls::Context;
use layerfs_bridge::control::SdkValue;
use layerfs_home::outcome::Outcome;

/// Its table entry.
pub(crate) const CALL: Call = call(
    "branch",
    "list",
    &[a("project", Project)],
    &PAGE,
    "a project's Branches",
    run,
);

/// Runs `branch.list`.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let (home, events) = context.parts();
    layerfs_store::calls::branch_list(home, events, args)
}
