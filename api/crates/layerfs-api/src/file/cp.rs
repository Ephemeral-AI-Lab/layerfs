//! `file.cp`: copy a path; one Commit.
use crate::calls::table::Kind::{Name, Project, Text};
use crate::calls::table::{a, call, Call, MESSAGE};
use crate::calls::Context;
use layerfs_bridge::control::SdkValue;
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
    let (home, events) = context.parts();
    layerfs_store::calls::file_cp(home, events, args)
}
