//! `file.diff`: the difference between two files.
use crate::calls::table::Kind::{Name, Project, Text};
use crate::calls::table::{a, call, opt, Call};
use crate::calls::Context;
use layerfs_bridge::control::SdkValue;
use layerfs_home::outcome::Outcome;

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
    let (home, events) = context.parts();
    layerfs_store::calls::file_diff(home, events, args)
}
