//! `file.cp`: copy a path; one Commit.
use layerfs_bridge::control::SdkValue;
use layerfs_control::calls::table::Kind::{Name, Project, Text};
use layerfs_control::calls::table::{a, call, Call, MESSAGE};
use layerfs_control::calls::Context;
use layerfs_home::outcome::Outcome;

/// Its table entry.
pub(crate) const CALL: Call = call(
    "file",
    "cp",
    &[
        a("project", Project),
        a("branch", Name),
        a("from_path", Text),
        a("to_path", Text),
    ],
    &[MESSAGE],
    "copy a path; one Commit",
    run,
);

/// Runs `file.cp`.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    layerfs_control::store::calls::run(context, "file", "cp", args)
}
