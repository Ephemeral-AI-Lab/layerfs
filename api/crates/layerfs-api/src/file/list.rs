//! `file.list`: a folder's entries.
use crate::calls::table::Kind::{Name, Project, Text};
use crate::calls::table::{a, call, opt, Call, PAGE};
use crate::calls::Context;
use layerfs_bridge::control::SdkValue;
use layerfs_home::outcome::Outcome;

/// Its table entry.
pub(crate) const CALL: Call = call(
    "file",
    "list",
    &[
        a("project", Project),
        a("branch", Name),
        a("path", Text),
        opt("commit", Name),
    ],
    &PAGE,
    "a folder's entries",
    run,
);

/// Runs `file.list`.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let (home, events) = context.parts();
    layerfs_store::calls::file_list(home, events, args)
}
