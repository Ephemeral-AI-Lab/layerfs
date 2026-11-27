//! `workspace.list`: every mounted Workspace, or one Sandbox's, from
//! Workspace records (call surface 2.0); it asks no daemon.
use crate::dispatch::table::Kind::{Int, Name, Text};
use crate::dispatch::table::{call, opt, Call};
use crate::dispatch::{Context, Form, Paging, Part};
use layerfs_home::outcome::Outcome;
use layerfs_protocol::sdk::SdkValue;
use layerfs_workspace::list::{list, ListRequest};

/// One Workspace: its handle and Branch.
const ITEM: Form = Form::of(
    &["workspace", "branch"],
    &[],
    &[
        "workspace",
        "branch",
        "sandbox",
        "project",
        "path",
        "mounted_at",
    ],
);
/// A Sandbox the list could not include.
const NOT_LISTED: Form = Form::of(
    &["sandbox", "message"],
    &["blocked_by", "reason"],
    &["sandbox", "blocked_by", "reason", "message"],
);
const FORM: Form = Form::of(
    &["items"],
    &["next_cursor", "not_listed"],
    &["items", "next_cursor", "not_listed"],
)
.parts(&[
    ("items", Part::Rows(&ITEM)),
    ("not_listed", Part::Rows(&NOT_LISTED)),
]);

/// Its table entry.
pub(crate) const CALL: Call = call(
    "workspace",
    "list",
    &[],
    &[
        opt("sandbox", Name),
        opt("limit", Int),
        opt("cursor", Text).redact(),
    ],
    "every mounted Workspace, or one Sandbox's",
    run,
)
.form(&FORM)
.paged(Paging::Cursor)
.agent();

/// Runs `workspace.list`.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let request = ListRequest {
        sandbox: args.get_text("sandbox").map(str::to_owned),
        limit: args.get_int("limit"),
        cursor: args.get_text("cursor").map(str::to_owned),
    };
    list(&context.home, &request)
}
