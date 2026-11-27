//! `file.list`: a folder's entries.
use super::text;
use crate::calls::scope::Rule;
use crate::calls::table::Kind::{Int, Name, Project, Text};
use crate::calls::table::{a, call, opt, Call};
use crate::calls::{Context, Form, Paging, Part};
use layerfs_protocol::provision::install::SdkValue;
use layerfs_home::outcome::Outcome;
use layerfs_store::calls::file::{list, FileList};

/// One entry: no size or mode (the listing reads neither).
const ROW: Form = Form::of(&["path", "type"], &[], &["path", "type", "inode"]);
/// `skipped_hidden` counts the entries `project hide-path` hides.
const FORM: Form = Form::of(
    &["items"],
    &["next_cursor"],
    &[
        "items",
        "next_cursor",
        "commit",
        "root",
        "path",
        "skipped_hidden",
    ],
)
.parts(&[("items", Part::Rows(&ROW))]);

/// Its table entry.
pub(crate) const CALL: Call = call(
    "file",
    "list",
    &[
        a("project", Project),
        a("branch", Name),
        a("path", Text).redact_path(),
    ],
    &[
        opt("at", Name),
        opt("limit", Int),
        opt("cursor", Text).redact(),
    ],
    "a folder's entries",
    run,
)
.scoped(Rule::File(&["path"]))
.form(&FORM)
.paged(Paging::Cursor)
.agent();

/// Runs `file.list`.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let request = FileList {
        project: text(args, "project").unwrap_or_default(),
        branch: text(args, "branch").unwrap_or_default(),
        at: text(args, "at"),
        path: text(args, "path").unwrap_or_default(),
        limit: args.get_int("limit"),
        cursor: text(args, "cursor"),
    };
    let (home, events) = context.parts();
    list(home, events, &request)
}
