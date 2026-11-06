//! `file.grep`: lines under `path` matching a regular expression.
use layerfs_bridge::control::SdkValue;
use layerfs_control::calls::table::Kind::{Name, Project, Text};
use layerfs_control::calls::table::{a, call, opt, Call, PAGE};
use layerfs_control::calls::Context;
use layerfs_home::outcome::Outcome;

/// Its table entry.
pub(crate) const CALL: Call = call(
    "file",
    "grep",
    &[
        a("project", Project),
        a("branch", Name),
        a("path", Text),
        a("pattern", Text),
        opt("commit", Name),
    ],
    &PAGE,
    "lines under `path` matching a regular expression",
    run,
);

/// Runs `file.grep`.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    layerfs_control::store::calls::run(context, "file", "grep", args)
}
