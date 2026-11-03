//! `branch.fork`: a new Branch from another Branch's head or Commit.
use layerfs_bridge::control::SdkValue;
use layerfs_control::calls::table::Kind::{Name, Project};
use layerfs_control::calls::table::{a, call, opt, Call};
use layerfs_control::calls::Context;
use layerfs_control::common::outcome::Outcome;

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
    layerfs_control::store::calls::run(context, "branch", "fork", args)
}
