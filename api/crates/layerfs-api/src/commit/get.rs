//! `commit.get`: one Commit of a Branch.
use layerfs_bridge::control::SdkValue;
use layerfs_control::calls::table::Kind::{Name, Project};
use layerfs_control::calls::table::{a, call, Call};
use layerfs_control::calls::Context;
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
    layerfs_control::store::calls::run(context, "commit", "get", args)
}
