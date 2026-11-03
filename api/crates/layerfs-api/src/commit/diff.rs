//! `commit.diff`: files that differ between two Commits.
use layerfs_bridge::control::SdkValue;
use layerfs_control::calls::table::Kind::{Name, Project};
use layerfs_control::calls::table::{a, call, Call, DIFF};
use layerfs_control::calls::Context;
use layerfs_control::common::outcome::Outcome;

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
    layerfs_control::store::calls::run(context, "commit", "diff", args)
}
