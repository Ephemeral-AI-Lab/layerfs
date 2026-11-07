//! `workspace.exec`: run one bash command in a Workspace.
use crate::calls::table::Kind::{Bool, Function, Text};
use crate::calls::table::{a, call, opt, Call, MESSAGE, PROGRESS};
use crate::calls::Context;
use layerfs_bridge::control::SdkValue;
use layerfs_home::outcome::Outcome;
use layerfs_sandbox::host::Message;
use layerfs_sandbox::workspace::exec::{exec, ExecRequest};

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
        opt("exec_id", Text),
        PROGRESS,
    ],
    "run one bash command in a Workspace",
    run,
);

/// Runs `workspace.exec`: its side's answer.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let request = ExecRequest {
        ws: args.get_text("ws").unwrap_or("").to_owned(),
        command: args.get_text("command").map(str::to_owned),
        // a missing message is null
        message: match Message::read(args.get("message")) {
            Message::Absent => Message::Null,
            message => message,
        },
        exec_id: args.get_text("exec_id").map(str::to_owned),
    };
    let (home, events) = context.parts();
    exec(home, events, &request)
}
