//! `branch.list`: a project's Branches.
use layerfs_bridge::control::SdkValue;
use layerfs_control::calls::table::Kind::Project;
use layerfs_control::calls::table::{a, call, Call, PAGE};
use layerfs_control::calls::Context;
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
    layerfs_control::store::calls::run(context, "branch", "list", args)
}
