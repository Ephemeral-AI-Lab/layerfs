//! `sandbox.list`: every Sandbox in the home, or one project's or one
//! machine's, from records, paged by cursor.
use super::get::facts;
use crate::calls::table::Kind::{Int, Name, Project, Text};
use crate::calls::table::{call, opt, Call};
use crate::calls::{Context, Form, Paging, Part};
use layerfs_home::outcome::Outcome;
use layerfs_protocol::sdk::SdkValue;
use layerfs_sandbox::family::{list, ListRequest, ListedSandbox};

/// The bytes past which a page ends early with a cursor (`list_shape`
/// `page_bytes`), counted on each row's full map.
const PAGE_BYTES: usize = 65536;

/// One Workspace the home records.
const WORKSPACE: Form = Form::of(&["workspace", "branch"], &[], &["workspace", "branch"]);
/// One Sandbox: its name and project, its Workspaces, its state when not
/// ready, then what its record holds.
const ITEM: Form = Form::of(
    &["sandbox", "project"],
    &[
        "workspaces",
        "workspaces_not_listed",
        "state",
        "op",
        "blocked_by",
        "reason",
        "fix",
    ],
    &[
        "sandbox",
        "project",
        "workspaces",
        "workspaces_not_listed",
        "state",
        "op",
        "blocked_by",
        "reason",
        "fix",
        "message",
        "machine",
        "platform",
        "image",
        "created_at",
    ],
)
.parts(&[("workspaces", Part::Rows(&WORKSPACE))]);
const FORM: Form = Form::of(&["items"], &["next_cursor"], &["items", "next_cursor"])
    .parts(&[("items", Part::Rows(&ITEM))]);

/// Its table entry.
pub(crate) const CALL: Call = call(
    "sandbox",
    "list",
    &[],
    &[
        opt("project", Project),
        opt("machine", Name),
        opt("limit", Int),
        opt("cursor", Text).redact(),
    ],
    "every Sandbox in the home, or one project's or one machine's",
    run,
)
.form(&FORM)
.paged(Paging::Cursor);

/// Runs `sandbox.list`: one page of the Sandboxes by name, each with its
/// state from its record and its machine's; no machine or daemon is asked.
/// A page ends after `limit` rows (100 by default), or early at 64 KiB,
/// with a `next_cursor` that carries the filters.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let text = |name: &str| args.get_text(name).map(str::to_owned);
    let request = ListRequest {
        project: text("project"),
        machine: text("machine"),
        limit: args.get_int("limit"),
        cursor: text("cursor"),
    };
    let listed = list(&context.home, &request)?;
    let mut items = Vec::new();
    let mut bytes = 0;
    let mut cut = false;
    for sandbox in &listed.items {
        let row = item(sandbox);
        bytes += layerfs_home::json::print(&row).len();
        if bytes > PAGE_BYTES && !items.is_empty() {
            cut = true;
            break;
        }
        items.push((sandbox.record.name(), row));
    }
    let next = match (items.last(), cut || listed.more) {
        (Some((last, _)), true) => SdkValue::Text(listed.cursor(last)),
        _ => SdkValue::Null,
    };
    let rows = items.into_iter().map(|(_, row)| row).collect();
    Ok(SdkValue::map([
        ("items", SdkValue::List(rows)),
        ("next_cursor", next),
    ]))
}

/// One Sandbox of a page, as the row names it.
fn item(sandbox: &ListedSandbox) -> SdkValue {
    let workspaces = (sandbox.workspaces.as_ref()).map(|found| {
        let row = |(workspace, branch): &(String, Option<String>)| {
            SdkValue::map([
                ("workspace", SdkValue::text(workspace)),
                ("branch", SdkValue::opt_text(branch.clone())),
            ])
        };
        SdkValue::List(found.iter().map(row).collect())
    });
    let mut fields = vec![
        ("workspaces", workspaces.unwrap_or_default()),
        ("workspaces_not_listed", SdkValue::Bool(sandbox.not_listed)),
        ("message", SdkValue::opt_text(sandbox.message.clone())),
    ];
    let shown = [
        "sandbox",
        "project",
        "machine",
        "platform",
        "image",
        "created_at",
    ];
    fields.extend(
        facts(&sandbox.record)
            .into_iter()
            .filter(|(name, _)| shown.contains(name)),
    );
    let row = SdkValue::Map(
        (fields.into_iter())
            .map(|(name, value)| (name.to_owned(), value))
            .collect(),
    );
    sandbox.state.write(row)
}
