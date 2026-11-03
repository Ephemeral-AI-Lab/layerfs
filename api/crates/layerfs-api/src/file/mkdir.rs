//! `file.mkdir`: make a folder; one Commit.
use layerfs_bridge::control::SdkValue;
use layerfs_control::calls::table::Kind::{Int, Name, Project, Text};
use layerfs_control::calls::table::{a, call, opt, Call, MESSAGE};
use layerfs_control::calls::Context;
use layerfs_control::common::outcome::Outcome;

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
    layerfs_control::store::calls::run(context, "file", "mkdir", args)
}
