//! `file.apply`: apply a list of changes; one Commit.
use crate::calls::table::Kind::{Json, Name, Project};
use crate::calls::table::{a, call, Call, MESSAGE};
use crate::calls::Context;
use layerfs_bridge::control::SdkValue;
use layerfs_home::outcome::Outcome;

/// Its table entry.
pub(crate) const CALL: Call = call(
    "file",
    "apply",
    &[a("project", Project), a("branch", Name), a("changes", Json)],
    &[MESSAGE],
    "apply a list of changes; one Commit",
    run,
);

/// Runs `file.apply`.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let (home, events) = context.parts();
    layerfs_store::calls::file_apply(home, events, args)
}
