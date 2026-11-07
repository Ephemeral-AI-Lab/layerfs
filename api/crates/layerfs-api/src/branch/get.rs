//! `branch.get`: one Branch and its head Commit.
use crate::calls::table::Kind::{Name, Project};
use crate::calls::table::{a, call, Call};
use crate::calls::Context;
use layerfs_bridge::control::SdkValue;
use layerfs_home::outcome::Outcome;
use layerfs_store::calls::branch::{get, BranchGet};

/// Its table entry.
pub(crate) const CALL: Call = call(
    "branch",
    "get",
    &[a("project", Project), a("branch", Name)],
    &[],
    "one Branch and its head Commit",
    run,
);

/// Runs `branch.get`.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let text = |name: &str| args.get_text(name).map(str::to_owned);
    let request = BranchGet {
        project: text("project").unwrap_or_default(),
        branch: text("branch").unwrap_or_default(),
    };
    let (home, events) = context.parts();
    get(home, events, &request)
}
