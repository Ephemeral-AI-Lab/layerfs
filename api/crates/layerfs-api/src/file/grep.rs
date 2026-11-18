//! `file.grep`: lines under a folder that contain a literal text.
use super::text;
use crate::calls::scope::Rule;
use crate::calls::table::Kind::{Bool, Int, Name, Project, Text};
use crate::calls::table::{a, call, opt, Call};
use crate::calls::{Context, Form, Paging, Part};
use layerfs_bridge::control::SdkValue;
use layerfs_home::outcome::Outcome;
use layerfs_store::calls::file::{grep, FileGrep};

/// One matching line.
const ROW: Form = Form::of(
    &["path", "line", "text"],
    &["line_truncated"],
    &["path", "line", "text", "line_truncated"],
);
/// `skipped_binary` counts the files that are not text.
const FORM: Form = Form::of(
    &["items"],
    &["next_cursor"],
    &[
        "items",
        "next_cursor",
        "commit",
        "root",
        "path",
        "skipped_binary",
    ],
)
.parts(&[("items", Part::Rows(&ROW))]);

/// Its table entry.
pub(crate) const CALL: Call = call(
    "file",
    "grep",
    &[
        a("project", Project),
        a("branch", Name),
        a("pattern", Text).redact(),
        opt("path", Text).redact_path(),
    ],
    &[
        opt("include", Text).redact(),
        opt("exclude", Text).redact(),
        opt("ignore_case", Bool),
        opt("at", Name),
        opt("limit", Int),
        opt("cursor", Text).redact(),
    ],
    "lines under a folder that contain a literal text",
    run,
)
.scoped(Rule::File(&["path"]))
.form(&FORM)
.paged(Paging::Cursor)
.agent();

/// Runs `file.grep`.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let request = FileGrep {
        project: text(args, "project").unwrap_or_default(),
        branch: text(args, "branch").unwrap_or_default(),
        pattern: text(args, "pattern").unwrap_or_default(),
        path: text(args, "path"),
        include: text(args, "include"),
        exclude: text(args, "exclude"),
        ignore_case: args.get_bool("ignore_case"),
        at: text(args, "at"),
        limit: args.get_int("limit"),
        cursor: text(args, "cursor"),
    };
    let (home, events) = context.parts();
    grep(home, events, &request)
}
