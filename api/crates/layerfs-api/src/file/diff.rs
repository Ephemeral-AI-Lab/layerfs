//! `file.diff`: the difference between two files.
use crate::calls::table::Kind::{Name, Project, Text};
use crate::calls::table::{a, call, opt, Call};
use crate::calls::Context;
use layerfs_bridge::control::SdkValue;
use layerfs_home::outcome::Outcome;
use layerfs_store::calls::file::{diff, FileDiff, Side};

/// Its table entry.
pub(crate) const CALL: Call = call(
    "file",
    "diff",
    &[
        a("project", Project),
        a("left_branch", Name),
        a("left_path", Text),
        a("right_branch", Name),
        a("right_path", Text),
    ],
    &[opt("left_commit", Name), opt("right_commit", Name)],
    "the difference between two files",
    run,
);

/// Runs `file.diff`.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let text = |name: &str| args.get_text(name).map(str::to_owned);
    let side = |at: &str| Side {
        branch: text(&format!("{at}_branch")),
        commit: text(&format!("{at}_commit")),
        path: text(&format!("{at}_path")),
    };
    let request = FileDiff {
        project: text("project").unwrap_or_default(),
        left: side("left"),
        right: side("right"),
    };
    let (home, events) = context.parts();
    diff(home, events, &request)
}
