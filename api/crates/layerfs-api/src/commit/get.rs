//! `commit.get`: one Commit of a Branch.
use crate::calls::table::Kind::{Name, Project};
use crate::calls::table::{a, call, Call};
use crate::calls::Context;
use layerfs_bridge::control::SdkValue;
use layerfs_home::outcome::Outcome;
use layerfs_store::calls::commit::{get, CommitGet};

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
    let text = |name: &str| args.get_text(name).map(str::to_owned);
    let request = CommitGet {
        project: text("project").unwrap_or_default(),
        branch: text("branch").unwrap_or_default(),
        commit: text("commit").unwrap_or_default(),
    };
    let (home, events) = context.parts();
    get(home, events, &request)
}
