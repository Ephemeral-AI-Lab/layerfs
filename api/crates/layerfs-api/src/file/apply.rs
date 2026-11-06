//! `file.apply`: apply a list of changes; one Commit.
use layerfs_bridge::control::SdkValue;
use layerfs_control::calls::table::Kind::{Json, Name, Project};
use layerfs_control::calls::table::{a, call, Call, MESSAGE};
use layerfs_control::calls::Context;
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
    layerfs_control::store::calls::run(context, "file", "apply", args)
}
