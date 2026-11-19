//! `project.list`: every project in the home, or one Store's.
use crate::calls::table::Kind::{Int, Name, Text};
use crate::calls::table::{call, opt, Call};
use crate::calls::{Context, Form, Paging, Part, Section};
use layerfs_bridge::control::SdkValue;
use layerfs_home::names::StoreName;
use layerfs_home::outcome::Outcome;
use layerfs_store::project::read::ProjectList;

/// One project: its state when not ready, with that state's companion;
/// `commit` is main's head.
const ITEM: Form = Form::of(
    &["project"],
    &["state", "op", "blocked_by", "reason", "fix"],
    &[
        "project",
        "state",
        "op",
        "blocked_by",
        "reason",
        "fix",
        "store",
        "engine",
        "machine",
        "commit",
        "created_at",
    ],
);
/// Its answer: one page of items, and the next page's cursor.
const FORM: Form = Form::of(&["items"], &["next_cursor"], &["items", "next_cursor"])
    .parts(&[("items", Part::Rows(&ITEM))]);

/// Its table entry.
pub(crate) const CALL: Call = call(
    "project",
    "list",
    &[],
    &[
        opt("store", Name),
        opt("limit", Int),
        opt("cursor", Text).redact(),
    ],
    "every project in the home, or one Store's",
    run,
)
.form(&FORM)
.with(&[Section::item("stats", &["size"])])
.paged(Paging::Cursor);

/// Runs `project.list`: sizes are read only when `stats` was asked for.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let request = ProjectList {
        store: args.get_text("store").map(StoreName::parse).transpose()?,
        limit: args.get_int("limit"),
        cursor: args.get_text("cursor").map(str::to_owned),
        stats: context.with("stats"),
    };
    layerfs_control::project::list(&context.home, &request)
}
