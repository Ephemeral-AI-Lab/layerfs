//! `workspace.status`: a Workspace's changes since its base.
use crate::calls::table::Kind::Text;
use crate::calls::table::{a, call, Call};
use crate::calls::Context;
use layerfs_bridge::control::SdkValue;
use layerfs_home::outcome::Outcome;

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
    let (home, events) = context.parts();
    layerfs_sandbox::workspace::calls::status(home, events, args)
}
