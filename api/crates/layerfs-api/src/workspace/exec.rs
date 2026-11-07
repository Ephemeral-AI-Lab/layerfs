//! `workspace.exec`: run one bash command in a Workspace.
use crate::calls::table::Kind::{Bool, Function, Text};
use crate::calls::table::{a, call, opt, Call, MESSAGE};
use crate::calls::Context;
use layerfs_bridge::control::SdkValue;
use layerfs_home::outcome::Outcome;
use layerfs_sandbox::workspace::calls::{exec, ExecRequest};

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

/// Runs `workspace.exec`: its side's answer.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let request = ExecRequest {
        ws: super::ws(args),
        command: args.get_text("command").map(str::to_owned),
        message: super::message(args),
    };
    let (home, events) = context.parts();
    exec(home, events, &request)
}
