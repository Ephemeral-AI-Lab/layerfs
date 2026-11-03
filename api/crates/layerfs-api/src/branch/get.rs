//! `branch.get`: one Branch and its head Commit.
use layerfs_bridge::control::SdkValue;
use layerfs_control::calls::table::Kind::{Name, Project};
use layerfs_control::calls::table::{a, call, Call};
use layerfs_control::calls::Context;
use layerfs_control::common::outcome::Outcome;

/// Its table entry.
pub(crate) const CALL: Call = call(
    "branch",
    "get",
    &[a("project", Project), a("branch", Name)],
    &[],
    "one Branch and its head Commit",
    run,
);

/// Runs `branch.get`.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    layerfs_control::store::calls::run(context, "branch", "get", args)
}
