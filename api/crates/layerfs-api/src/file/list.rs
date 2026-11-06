//! `file.list`: a folder's entries.
use layerfs_bridge::control::SdkValue;
use layerfs_control::calls::table::Kind::{Name, Project, Text};
use layerfs_control::calls::table::{a, call, opt, Call, PAGE};
use layerfs_control::calls::Context;
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
    layerfs_control::store::calls::run(context, "file", "list", args)
}
