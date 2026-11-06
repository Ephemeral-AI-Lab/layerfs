//! `file.read`: read a file.
use crate::calls::table::Kind::{Int, Name, Project, Text};
use crate::calls::table::{a, call, opt, Call};
use crate::calls::Context;
use layerfs_bridge::control::SdkValue;
use layerfs_home::outcome::Outcome;

/// Its table entry.
pub(crate) const CALL: Call = call(
    "file",
    "read",
    &[
        a("project", Project),
        a("branch", Name),
        a("path", Text),
        opt("commit", Name),
    ],
    &[opt("start_line", Int), opt("end_line", Int)],
    "read a file",
    run,
);

/// Runs `file.read`.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let (home, events) = context.parts();
    layerfs_store::calls::file_read(home, events, args)
}
