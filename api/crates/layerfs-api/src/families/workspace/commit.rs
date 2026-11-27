//! `workspace.commit`: commit a Workspace's changes to its Branch.
use crate::dispatch::guard::scope::Rule;
use crate::dispatch::table::Kind::{Bool, Name, Text};
use crate::dispatch::table::{a, call, opt, Call, MESSAGE};
use crate::dispatch::{Context, Form, Policy};
use layerfs_home::reply::outcome::Outcome;
use layerfs_protocol::sdk::SdkValue;
use layerfs_sandbox::route::machine::Message;
use layerfs_workspace::answers::committed;
use layerfs_workspace::route::send::{commit, CommitRequest};

const FORM: Form = Form::of(
    &["commit"],
    &["unchanged", "merged", "overwritten", "overwritten_count"],
    &[
        "commit",
        "unchanged",
        "merged",
        "overwritten",
        "overwritten_count",
        "outcome",
        "branch",
        "parent",
        "root",
        "message",
        "created_at",
        "merged_from",
        "displaced",
    ],
)
.parts(&[("overwritten", crate::dispatch::Part::Scalars("path"))]);

/// Its table entry.
pub(crate) const CALL: Call = call(
    "workspace",
    "commit",
    &[a("workspace", Text)],
    &[MESSAGE, opt("expected", Name), opt("force", Bool)],
    "commit a Workspace's changes to its Branch",
    run,
)
.form(&FORM)
.agent()
.scoped(Rule::Workspace)
.policy(Policy::Workspace("workspace"))
.saves();

/// Runs `workspace.commit`.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let request = CommitRequest {
        ws: args.get_text("workspace").unwrap_or("").to_owned(),
        // a missing message is null
        message: match Message::read(args.get("message")) {
            Message::Absent => Message::Null,
            message => message,
        },
        force: args.get("force").map(|_| args.get_bool("force")),
        expected: args.get_text("expected").map(str::to_owned),
    };
    let (home, events) = context.parts();
    Ok(committed(&commit(home, events, &request)?))
}
