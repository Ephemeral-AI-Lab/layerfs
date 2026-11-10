//! `branch.get_commit`: one saved branch record, using a complete immutable ID.
use crate::calls::{
    table::{
        a, call, Call,
        Kind::{Name, Project},
    },
    Context,
};
use layerfs_bridge::control::SdkValue;
use layerfs_home::outcome::Outcome;
use layerfs_store::calls::branch::{get_commit, BranchGetCommit};
/// Its table entry.
pub(crate) const CALL: Call = call(
    "branch",
    "get_commit",
    &[a("project", Project), a("branch", Name), a("commit", Name)],
    &[],
    "one saved Commit or main publication",
    run,
);
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let request = BranchGetCommit {
        project: args.get_text("project").unwrap_or_default().into(),
        branch: args.get_text("branch").unwrap_or_default().into(),
        commit: args.get_text("commit").unwrap_or_default().into(),
    };
    let (home, events) = context.parts();
    get_commit(home, events, &request)
}
