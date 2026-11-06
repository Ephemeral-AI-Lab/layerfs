//! `file.write`: write a file; one Commit.
use crate::calls::table::Kind::{Content, Int, Name, Project, Text};
use crate::calls::table::{a, call, opt, Call, MESSAGE};
use crate::calls::Context;
use layerfs_bridge::control::SdkValue;
use layerfs_home::outcome::Outcome;

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
    let (home, events) = context.parts();
    layerfs_store::calls::file_write(home, events, args)
}
