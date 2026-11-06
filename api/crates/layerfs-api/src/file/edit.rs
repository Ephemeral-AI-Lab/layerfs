//! `file.edit`: replace one unique text in a file; one Commit.
use layerfs_bridge::control::SdkValue;
use layerfs_control::calls::table::Kind::{Name, Project, Text};
use layerfs_control::calls::table::{a, call, opt, Call, MESSAGE};
use layerfs_control::calls::Context;
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
    layerfs_control::store::calls::run(context, "file", "edit", args)
}
