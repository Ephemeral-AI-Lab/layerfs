//! `workspace.unmount`: end a Workspace.
use crate::calls::table::Kind::{Bool, Text};
use crate::calls::table::{a, call, opt, Call};
use crate::calls::Context;
use layerfs_bridge::control::SdkValue;
use layerfs_home::outcome::Outcome;
use layerfs_sandbox::workspace::calls::{unmount, UnmountRequest};

/// Its table entry.
pub(crate) const CALL: Call = call(
    "workspace",
    "unmount",
    &[a("ws", Text)],
    &[opt("discard", Bool), opt("force", Bool)],
    "end a Workspace",
    run,
);

/// Runs `workspace.unmount`: its side's answer.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let request = UnmountRequest {
        ws: args.get_text("ws").unwrap_or("").to_owned(),
        discard: args.get("discard").map(|_| args.get_bool("discard")),
        force: args.get("force").map(|_| args.get_bool("force")),
    };
    let (home, events) = context.parts();
    unmount(home, events, &request)
}
