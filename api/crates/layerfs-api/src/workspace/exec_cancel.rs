//! `workspace.exec_cancel`: end a running command (compute-plane 2.0 §5.1).
use crate::calls::table::Kind::{Int, Text};
use crate::calls::table::{a, call, opt, Call};
use crate::calls::Context;
use layerfs_bridge::control::SdkValue;
use layerfs_home::outcome::Outcome;
use layerfs_sandbox::workspace::exec::{exec_cancel, ExecCancelRequest};

/// Its table entry.
pub(crate) const CALL: Call = call(
    "workspace",
    "exec_cancel",
    &[a("ws", Text), a("exec_id", Text)],
    &[opt("grace_ms", Int)],
    "end a running command: SIGTERM, then SIGKILL after grace_ms",
    run,
);

/// Runs `workspace.exec_cancel`: `{ outcome, signal? }`.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let request = ExecCancelRequest {
        ws: args.get_text("ws").unwrap_or("").to_owned(),
        exec_id: args.get_text("exec_id").unwrap_or("").to_owned(),
        grace_ms: args.get_int("grace_ms"),
    };
    let (home, events) = context.parts();
    exec_cancel(home, events, &request)
}
