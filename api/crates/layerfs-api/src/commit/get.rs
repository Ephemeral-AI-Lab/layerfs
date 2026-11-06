//! `commit.get`: one Commit of a Branch.
use crate::calls::table::Kind::{Name, Project};
use crate::calls::table::{a, call, Call};
use crate::calls::Context;
use layerfs_bridge::control::SdkValue;
use layerfs_home::outcome::Outcome;

/// Its table entry.
pub(crate) const CALL: Call = call(
    "commit",
    "get",
    &[a("project", Project), a("branch", Name), a("commit", Name)],
    &[],
    "one Commit of a Branch",
    run,
);

/// Runs `commit.get`.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let (home, events) = context.parts();
    layerfs_store::calls::commit_get(home, events, args)
}
