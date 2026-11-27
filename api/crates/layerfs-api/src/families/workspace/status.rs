//! `workspace.status`: a Workspace's uncommitted changes, and whether its
//! Branch moved.
use crate::dispatch::guard::scope::Rule;
use crate::dispatch::table::Kind::Text;
use crate::dispatch::table::{a, call, opt, Call};
use crate::dispatch::{Context, Form, Part};
use layerfs_home::reply::outcome::Outcome;
use layerfs_protocol::sdk::SdkValue;
use layerfs_workspace::answers::status as answered;
use layerfs_workspace::route::send::{status, StatusRequest};

/// One change; a file is the usual `type`.
const CHANGE: Form = Form::of(&["path", "change"], &["type"], &["path", "change", "type"])
    .usual(&[("type", "file")]);
/// The overlay's own counts.
const OVERLAY: Form = Form::of(&[], &[], &["inode_count", "entry_count"]);
const FORM: Form = Form::of(
    &["changes"],
    &[
        "changes_count",
        "partial",
        "branch_moved",
        "state",
        "op",
        "reason",
        "fix",
    ],
    &[
        "changes",
        "changes_count",
        "partial",
        "branch_moved",
        "state",
        "op",
        "reason",
        "fix",
        "branch",
        "commit",
        "base_layer",
        "branch_head",
        "activity",
        "overlay",
        "pending_merge",
    ],
)
.parts(&[
    ("changes", Part::Rows(&CHANGE)),
    ("overlay", Part::Rows(&OVERLAY)),
]);

/// Its table entry.
pub(crate) const CALL: Call = call(
    "workspace",
    "status",
    &[a("workspace", Text)],
    &[opt("path", Text).redact_path()],
    "a Workspace's uncommitted changes, and whether its Branch moved",
    run,
)
.form(&FORM)
.agent()
.scoped(Rule::Workspace);

/// Runs `workspace.status`.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let ws = args.get_text("workspace").unwrap_or("").to_owned();
    let request = StatusRequest {
        ws: ws.clone(),
        path: args.get_text("path").map(str::to_owned),
    };
    let (home, events) = context.parts();
    Ok(answered(&status(home, events, &request)?, &ws))
}
