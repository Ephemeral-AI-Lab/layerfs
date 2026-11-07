//! `commit.list`: a Branch's Commits, newest first.
use crate::calls::table::Kind::{Name, Project};
use crate::calls::table::{a, call, Call, PAGE};
use crate::calls::Context;
use layerfs_bridge::control::SdkValue;
use layerfs_home::outcome::Outcome;
use layerfs_store::calls::commit::{list, CommitList};
use layerfs_store::calls::Page;

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
    let text = |name: &str| args.get_text(name).map(str::to_owned);
    let request = CommitList {
        project: text("project").unwrap_or_default(),
        branch: text("branch").unwrap_or_default(),
        page: Page {
            size: args.get_int("page_size"),
            pos: args.get_int("page_pos"),
        },
    };
    let (home, events) = context.parts();
    list(home, events, &request)
}
