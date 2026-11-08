//! `branch.fork`: a new Branch from another Branch's head or Commit.
use crate::calls::table::Kind::{Name, Project};
use crate::calls::table::{a, call, opt, Call};
use crate::calls::Context;
use layerfs_bridge::control::SdkValue;
use layerfs_home::outcome::Outcome;
use layerfs_store::calls::branch::{fork, BranchFork};

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
    let text = |name: &str| args.get_text(name).map(str::to_owned);
    let request = BranchFork {
        project: text("project").unwrap_or_default(),
        name: text("name").unwrap_or_default(),
        from_branch: text("from_branch").unwrap_or_default(),
        from_commit: text("from_commit"),
    };
    let (home, events) = context.parts();
    fork(home, events, &request)
}
