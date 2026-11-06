//! `branch.fork`: a new Branch from another Branch's head or Commit.
use crate::calls::table::Kind::{Name, Project};
use crate::calls::table::{a, call, opt, Call};
use crate::calls::Context;
use layerfs_bridge::control::SdkValue;
use layerfs_home::outcome::Outcome;

/// Its table entry.
pub(crate) const CALL: Call = call(
    "branch",
    "fork",
    &[
        a("project", Project),
        a("name", Name),
        a("from_branch", Name),
        opt("from_commit", Name),
    ],
    &[],
    "a new Branch from another Branch's head or Commit",
    run,
);

/// Runs `branch.fork`.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let (home, events) = context.parts();
    layerfs_store::calls::branch_fork(home, events, args)
}
