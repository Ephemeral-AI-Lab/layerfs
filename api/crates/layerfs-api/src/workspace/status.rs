//! `workspace.status`: a Workspace's changes since its base.
use layerfs_bridge::control::SdkValue;
use layerfs_control::calls::table::Kind::Text;
use layerfs_control::calls::table::{a, call, Call};
use layerfs_control::calls::Context;
use layerfs_control::common::outcome::Outcome;

/// Its table entry.
pub(crate) const CALL: Call = call(
    "workspace",
    "status",
    &[a("ws", Text)],
    &[],
    "a Workspace's changes since its base",
    run,
);

/// Runs `workspace.status`.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    layerfs_control::workspace::calls::workspace(context, "status", args)
}
