//! `file.blame`: the Commit of each line.
use crate::calls::table::Kind::{Int, Name, Project, Text};
use crate::calls::table::{a, call, opt, Call};
use crate::calls::Context;
use layerfs_bridge::control::SdkValue;
use layerfs_home::outcome::Outcome;

/// Its table entry.
pub(crate) const CALL: Call = call(
    "file",
    "blame",
    &[
        a("project", Project),
        a("branch", Name),
        a("path", Text),
        opt("commit", Name),
    ],
    &[opt("from_line", Int)],
    "the Commit of each line",
    run,
);

/// Runs `file.blame`.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let (home, events) = context.parts();
    layerfs_store::calls::file_blame(home, events, args)
}
