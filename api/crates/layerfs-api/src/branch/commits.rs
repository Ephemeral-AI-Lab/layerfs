//! `branch.commits`: pinned saved Commit or publication history.
use crate::calls::{
    table::{
        a, call, opt, Call,
        Kind::{Int, Name, Project, Text},
    },
    Context,
};
use layerfs_bridge::control::SdkValue;
use layerfs_home::outcome::Outcome;
use layerfs_store::calls::branch::{commits, BranchCommits};
/// Its table entry.
pub(crate) const CALL: Call = call(
    "branch",
    "commits",
    &[a("project", Project), a("branch", Name)],
    &[
        opt("page", Int),
        opt("page_size", Int),
        opt("at_commit", Text),
    ],
    "saved branch commits or main publications, newest first",
    run,
);
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let request = BranchCommits {
        project: args.get_text("project").unwrap_or_default().into(),
        branch: args.get_text("branch").unwrap_or_default().into(),
        page: args.get_int("page"),
        page_size: args.get_int("page_size"),
        at_commit: args.get_text("at_commit").map(str::to_owned),
    };
    let (home, events) = context.parts();
    commits(home, events, &request)
}
