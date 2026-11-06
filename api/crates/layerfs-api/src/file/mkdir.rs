//! `file.mkdir`: make a folder; one Commit.
use crate::calls::table::Kind::{Int, Name, Project, Text};
use crate::calls::table::{a, call, opt, Call, MESSAGE};
use crate::calls::Context;
use layerfs_bridge::control::SdkValue;
use layerfs_home::outcome::Outcome;

/// Its table entry.
pub(crate) const CALL: Call = call(
    "file",
    "mkdir",
    &[a("project", Project), a("branch", Name), a("path", Text)],
    &[MESSAGE, opt("mode", Int)],
    "make a folder; one Commit",
    run,
);

/// Runs `file.mkdir`.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let (home, events) = context.parts();
    layerfs_store::calls::file_mkdir(home, events, args)
}
