//! `file.history`: the Commits that changed one path, newest first (was
//! `file.revision`).
use super::text;
use crate::calls::scope::Rule;
use crate::calls::table::Kind::{Int, Name, Project, Text};
use crate::calls::table::{a, call, opt, Call};
use crate::calls::{Context, Form, Paging, Part};
use layerfs_protocol::provision::install::SdkValue;
use layerfs_home::outcome::Outcome;
use layerfs_store::calls::file::{history, FileHistory};

/// One Commit that changed the path; `type`, `size` and `mode` are the
/// path's after it. A publication on main has no `message` or `created_at`.
const ROW: Form = Form::of(
    &["commit", "change"],
    &["message"],
    &[
        "commit",
        "change",
        "message",
        "parent",
        "created_at",
        "type",
        "size",
        "mode",
        "content",
        "metadata",
        "identity",
    ],
);
const FORM: Form = Form::of(
    &["items"],
    &["next_cursor"],
    &["items", "next_cursor", "commit", "root", "path"],
)
.parts(&[("items", Part::Rows(&ROW))]);

/// Its table entry.
pub(crate) const CALL: Call = call(
    "file",
    "history",
    &[
        a("project", Project),
        a("branch", Name),
        a("path", Text).redact_path(),
    ],
    &[
        opt("at", Name),
        opt("traversal", Text),
        opt("limit", Int),
        opt("cursor", Text).redact(),
    ],
    "the Commits that changed one path, newest first",
    run,
)
.scoped(Rule::History)
.form(&FORM)
.paged(Paging::Cursor);

/// Runs `file.history`.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let request = FileHistory {
        project: text(args, "project").unwrap_or_default(),
        branch: text(args, "branch").unwrap_or_default(),
        path: text(args, "path").unwrap_or_default(),
        at: text(args, "at"),
        traversal: text(args, "traversal"),
        limit: args.get_int("limit"),
        cursor: text(args, "cursor"),
    };
    let (home, events) = context.parts();
    history(home, events, &request)
}
