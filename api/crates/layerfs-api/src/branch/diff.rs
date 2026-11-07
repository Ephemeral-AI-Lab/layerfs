//! `branch.diff`: files that differ between two Branches' heads.
use crate::calls::table::Kind::{Name, Project};
use crate::calls::table::{a, call, Call, DIFF};
use crate::calls::Context;
use layerfs_bridge::control::SdkValue;
use layerfs_home::outcome::Outcome;
use layerfs_store::calls::branch::{diff, BranchDiff};
use layerfs_store::calls::{DiffOptions, Page};

/// Its table entry.
pub(crate) const CALL: Call = call(
    "branch",
    "diff",
    &[
        a("project", Project),
        a("from_branch", Name),
        a("to_branch", Name),
    ],
    &DIFF,
    "files that differ between two Branches' heads",
    run,
);

/// Runs `branch.diff`.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let text = |name: &str| args.get_text(name).map(str::to_owned);
    let request = BranchDiff {
        project: text("project").unwrap_or_default(),
        from_branch: text("from_branch").unwrap_or_default(),
        to_branch: text("to_branch").unwrap_or_default(),
        options: DiffOptions {
            page: Page {
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
