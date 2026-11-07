//! `workspace.status`: a Workspace's changes since its base.
use crate::calls::table::Kind::Text;
use crate::calls::table::{a, call, Call};
use crate::calls::Context;
use layerfs_bridge::control::SdkValue;
use layerfs_home::outcome::Outcome;
use layerfs_sandbox::workspace::calls::{status, StatusRequest};

/// Its table entry.
pub(crate) const CALL: Call = call(
    "workspace",
    "status",
    &[a("ws", Text)],
    &[],
    "a Workspace's changes since its base",
    run,
);

/// Runs `workspace.status`: its side's answer.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let request = StatusRequest {
        ws: super::ws(args),
    };
    let (home, events) = context.parts();
    status(home, events, &request)
}
