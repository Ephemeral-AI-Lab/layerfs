//! `workspace.unmount`: end a Workspace.
use layerfs_bridge::control::SdkValue;
use layerfs_control::calls::table::Kind::{Bool, Text};
use layerfs_control::calls::table::{a, call, opt, Call};
use layerfs_control::calls::Context;
use layerfs_control::common::outcome::Outcome;

/// Its table entry.
pub(crate) const CALL: Call = call(
    "workspace",
    "unmount",
    &[a("ws", Text)],
    &[opt("discard", Bool), opt("force", Bool), opt("kill", Text)],
    "end a Workspace",
    run,
);

/// Runs `workspace.unmount`.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    layerfs_control::workspace::calls::workspace(context, "unmount", args)
}
