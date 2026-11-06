//! `file.glob`: paths under `path` matching a glob.
use crate::calls::table::Kind::{Name, Project, Text};
use crate::calls::table::{a, call, opt, Call, PAGE};
use crate::calls::Context;
use layerfs_bridge::control::SdkValue;
use layerfs_home::outcome::Outcome;

/// Its table entry.
pub(crate) const CALL: Call = call(
    "file",
    "glob",
    &[
        a("project", Project),
        a("branch", Name),
        a("path", Text),
        a("pattern", Text),
        opt("commit", Name),
    ],
    &PAGE,
    "paths under `path` matching a glob",
    run,
);

/// Runs `file.glob`.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let (home, events) = context.parts();
    layerfs_store::calls::file_glob(home, events, args)
}
