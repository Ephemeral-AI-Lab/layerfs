//! `workspace.exec`: run one bash command in a Workspace.
use crate::calls::table::Kind::{Bool, Function, Text};
use crate::calls::table::{a, call, opt, Call, MESSAGE};
use crate::calls::Context;
use layerfs_bridge::control::SdkValue;
use layerfs_home::outcome::Outcome;

/// Its table entry.
pub(crate) const CALL: Call = call(
    "workspace",
    "exec",
    &[a("ws", Text), a("command", Text)],
    &[
        MESSAGE,
        opt("stream", Bool),
        opt("stdout", Function),
        opt("stderr", Function),
    ],
    "run one bash command in a Workspace",
    run,
);

/// Runs `workspace.exec`.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let (home, events) = context.parts();
    layerfs_sandbox::workspace::calls::exec(home, events, args)
}
