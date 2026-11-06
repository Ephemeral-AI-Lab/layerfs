//! `file.diff`: the difference between two files.
use layerfs_bridge::control::SdkValue;
use layerfs_control::calls::table::Kind::{Name, Project, Text};
use layerfs_control::calls::table::{a, call, opt, Call};
use layerfs_control::calls::Context;
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
    layerfs_control::store::calls::run(context, "file", "diff", args)
}
