//! `file.edit`: replace one unique text in a file; one Commit.
use crate::calls::table::Kind::{Name, Project, Text};
use crate::calls::table::{a, call, opt, Call, MESSAGE};
use crate::calls::Context;
use layerfs_bridge::control::SdkValue;
use layerfs_home::outcome::Outcome;

/// Its table entry.
pub(crate) const CALL: Call = call(
    "file",
    "edit",
    &[a("project", Project), a("branch", Name), a("path", Text)],
    &[opt("old", Text), opt("new", Text), MESSAGE],
    "replace one unique text in a file; one Commit",
    run,
);

/// Runs `file.edit`.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let (home, events) = context.parts();
    layerfs_store::calls::file_edit(home, events, args)
}
