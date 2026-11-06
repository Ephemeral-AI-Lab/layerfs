//! `branch.get`: one Branch and its head Commit.
use crate::calls::table::Kind::{Name, Project};
use crate::calls::table::{a, call, Call};
use crate::calls::Context;
use layerfs_bridge::control::SdkValue;
use layerfs_home::outcome::Outcome;

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
    let (home, events) = context.parts();
    layerfs_store::calls::branch_get(home, events, args)
}
