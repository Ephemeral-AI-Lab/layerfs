//! `file.write`: write a file; one Commit.
use layerfs_bridge::control::SdkValue;
use layerfs_control::calls::table::Kind::{Content, Int, Name, Project, Text};
use layerfs_control::calls::table::{a, call, opt, Call, MESSAGE};
use layerfs_control::calls::Context;
use layerfs_control::common::outcome::Outcome;

/// Its table entry.
pub(crate) const CALL: Call = call(
    "file",
    "write",
    &[
        a("project", Project),
        a("branch", Name),
        a("path", Text),
        a("content", Content),
    ],
    &[MESSAGE, opt("mode", Int)],
    "write a file; one Commit",
    run,
);

/// Runs `file.write`.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    layerfs_control::store::calls::run(context, "file", "write", args)
}
