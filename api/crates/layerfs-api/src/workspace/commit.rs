//! `workspace.commit`: commit a Workspace's changes to its Branch.
use layerfs_bridge::control::SdkValue;
use layerfs_control::calls::table::Kind::{Bool, Text};
use layerfs_control::calls::table::{a, call, opt, Call, MESSAGE};
use layerfs_control::calls::Context;
use layerfs_home::outcome::Outcome;

/// Its table entry.
pub(crate) const CALL: Call = call(
    "workspace",
    "commit",
    &[a("ws", Text)],
    &[MESSAGE, opt("force", Bool)],
    "commit a Workspace's changes to its Branch",
    run,
);

/// Runs `workspace.commit`.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    layerfs_control::workspace::calls::workspace(context, "commit", args)
}
