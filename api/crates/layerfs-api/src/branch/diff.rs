//! `branch.diff`: files that differ between two Branches' heads.
use crate::calls::table::Kind::{Bool, Int, Name, Project, Text};
use crate::calls::table::{a, call, opt, Call};
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
    &[
        opt("paths_only", Bool),
        opt("page_size", Int),
        opt("page_pos", Int),
        opt("cursor", Text),
        opt("page", Int),
        opt("from_commit", Text),
        opt("to_commit", Text),
        opt("path_prefix", Text),
        opt("include_attribution", Bool),
    ],
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
        from_commit: text("from_commit"),
        to_commit: text("to_commit"),
        path_prefix: text("path_prefix"),
        include_attribution: match args.get("include_attribution") {
            Some(SdkValue::Bool(v)) => Some(*v),
            _ => None,
        },
        options: DiffOptions {
            page: Page {
                size: args.get_int("page_size"),
                pos: args.get_int("page_pos"),
                number: args.get_int("page"),
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
