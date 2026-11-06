//! `commit.list`: a Branch's Commits, newest first.
use crate::calls::table::Kind::{Name, Project};
use crate::calls::table::{a, call, Call, PAGE};
use crate::calls::Context;
use layerfs_bridge::control::SdkValue;
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
    let (home, events) = context.parts();
    layerfs_store::calls::commit_list(home, events, args)
}
