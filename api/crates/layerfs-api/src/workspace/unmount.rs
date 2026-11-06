//! `workspace.unmount`: end a Workspace.
use crate::calls::table::Kind::{Bool, Text};
use crate::calls::table::{a, call, opt, Call};
use crate::calls::Context;
use layerfs_bridge::control::SdkValue;
use layerfs_home::outcome::Outcome;

/// Its table entry.
pub(crate) const CALL: Call = call(
    "workspace",
    "unmount",
    &[a("ws", Text)],
    &[opt("discard", Bool), opt("force", Bool)],
    "end a Workspace",
    run,
);

/// Runs `workspace.unmount`.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let (home, events) = context.parts();
    layerfs_sandbox::workspace::calls::unmount(home, events, args)
}
