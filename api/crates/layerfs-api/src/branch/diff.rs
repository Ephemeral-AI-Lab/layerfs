//! `branch.diff`: files that differ between two Branches' heads.
use layerfs_bridge::control::SdkValue;
use layerfs_control::calls::table::Kind::{Name, Project};
use layerfs_control::calls::table::{a, call, Call, DIFF};
use layerfs_control::calls::Context;
use layerfs_home::outcome::Outcome;

/// Its table entry.
pub(crate) const CALL: Call = call(
    "branch",
    "diff",
    &[
        a("project", Project),
        a("from_branch", Name),
        a("to_branch", Name),
    ],
    &DIFF,
    "files that differ between two Branches' heads",
    run,
);

/// Runs `branch.diff`.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    layerfs_control::store::calls::run(context, "branch", "diff", args)
}
