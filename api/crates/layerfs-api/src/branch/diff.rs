//! `branch.diff`: files that differ between two Branches' heads.
use crate::calls::table::Kind::{Name, Project};
use crate::calls::table::{a, call, Call, DIFF};
use crate::calls::Context;
use layerfs_bridge::control::SdkValue;
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
    let (home, events) = context.parts();
    layerfs_store::calls::branch_diff(home, events, args)
}
