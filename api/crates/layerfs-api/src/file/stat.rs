//! `file.stat`: a path's kind, size and mode.
use layerfs_bridge::control::SdkValue;
use layerfs_control::calls::table::Kind::{Name, Project, Text};
use layerfs_control::calls::table::{a, call, opt, Call};
use layerfs_control::calls::Context;
use layerfs_home::outcome::Outcome;

/// Its table entry.
pub(crate) const CALL: Call = call(
    "file",
    "stat",
    &[
        a("project", Project),
        a("branch", Name),
        a("path", Text),
        opt("commit", Name),
    ],
    &[],
    "a path's kind, size and mode",
    run,
);

/// Runs `file.stat`.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    layerfs_control::store::calls::run(context, "file", "stat", args)
}
