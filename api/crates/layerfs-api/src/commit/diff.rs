//! `commit.diff`: files that differ between two Commits.
use crate::calls::table::Kind::{Name, Project};
use crate::calls::table::{a, call, Call, DIFF};
use crate::calls::Context;
use layerfs_bridge::control::SdkValue;
use layerfs_home::outcome::Outcome;

/// Its table entry.
pub(crate) const CALL: Call = call(
    "commit",
    "diff",
    &[
        a("project", Project),
        a("from_commit", Name),
        a("to_commit", Name),
    ],
    &DIFF,
    "files that differ between two Commits",
    run,
);

/// Runs `commit.diff`.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let (home, events) = context.parts();
    layerfs_store::calls::commit_diff(home, events, args)
}
