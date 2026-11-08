//! `commit.diff`: files that differ between two Commits.
use crate::calls::table::Kind::{Name, Project};
use crate::calls::table::{a, call, Call, DIFF};
use crate::calls::Context;
use layerfs_bridge::control::SdkValue;
use layerfs_home::outcome::Outcome;
use layerfs_store::calls::commit::{diff, CommitDiff};
use layerfs_store::calls::{DiffOptions, Page};

/// Its table entry.
pub(crate) const CALL: Call = call(
    "commit",
    "diff",
    &[
        a("project", Project),
        a("from_commit", Name),
        a("to_commit", Name),
    ],
    &DIFF,
    "files that differ between two Commits",
    run,
);

/// Runs `commit.diff`.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let text = |name: &str| args.get_text(name).map(str::to_owned);
    let request = CommitDiff {
        project: text("project").unwrap_or_default(),
        from_commit: text("from_commit").unwrap_or_default(),
        to_commit: text("to_commit").unwrap_or_default(),
        options: DiffOptions {
            page: Page {
                number: None,
                size: args.get_int("page_size"),
                pos: args.get_int("page_pos"),
            },
            cursor: text("cursor"),
            paths_only: match args.get("paths_only") {
                Some(SdkValue::Bool(paths_only)) => Some(*paths_only),
                _ => None,
            },
        },
    };
    let (home, events) = context.parts();
    diff(home, events, &request)
}
