//! `workspace.exec`: run one bash command in a Workspace.
use layerfs_bridge::control::SdkValue;
use layerfs_control::calls::table::Kind::{Bool, Function, Text};
use layerfs_control::calls::table::{a, call, opt, Call, MESSAGE};
use layerfs_control::calls::Context;
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
    layerfs_control::workspace::calls::workspace(context, "exec", args)
}
