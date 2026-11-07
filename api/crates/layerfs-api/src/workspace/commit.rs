//! `workspace.commit`: commit a Workspace's changes to its Branch.
use crate::calls::table::Kind::{Bool, Text};
use crate::calls::table::{a, call, opt, Call, MESSAGE};
use crate::calls::Context;
use layerfs_bridge::control::SdkValue;
use layerfs_home::outcome::Outcome;
use layerfs_sandbox::host::Message;
use layerfs_sandbox::workspace::calls::{commit, CommitRequest};

/// Its table entry.
pub(crate) const CALL: Call = call(
    "workspace",
    "commit",
    &[a("ws", Text)],
    &[MESSAGE, opt("force", Bool)],
    "commit a Workspace's changes to its Branch",
    run,
);

/// Runs `workspace.commit`: its side's answer.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let request = CommitRequest {
        ws: args.get_text("ws").unwrap_or("").to_owned(),
        // a missing message is null
        message: match Message::read(args.get("message")) {
            Message::Absent => Message::Null,
            message => message,
        },
        force: args.get("force").map(|_| args.get_bool("force")),
    };
    let (home, events) = context.parts();
    commit(home, events, &request)
}
