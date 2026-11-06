//! `commit.list`: a Branch's Commits, newest first.
use layerfs_bridge::control::SdkValue;
use layerfs_control::calls::table::Kind::{Name, Project};
use layerfs_control::calls::table::{a, call, Call, PAGE};
use layerfs_control::calls::Context;
use layerfs_home::outcome::Outcome;

/// Its table entry.
pub(crate) const CALL: Call = call(
    "commit",
    "list",
    &[a("project", Project), a("branch", Name)],
    &PAGE,
    "a Branch's Commits, newest first",
    run,
);

/// Runs `commit.list`.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    layerfs_control::store::calls::run(context, "commit", "list", args)
}
