//! `workspace.commit`: commit a Workspace's changes to its Branch.
use crate::calls::table::Kind::{Bool, Text};
use crate::calls::table::{a, call, opt, Call, MESSAGE};
use crate::calls::Context;
use layerfs_bridge::control::SdkValue;
use layerfs_home::outcome::Outcome;
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
        ws: super::ws(args),
        message: super::message(args),
        force: super::flag(args, "force"),
    };
    let (home, events) = context.parts();
    commit(home, events, &request)
}
