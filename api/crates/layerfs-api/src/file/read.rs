//! `file.read`: read a text file, or a window of its lines.
use super::text;
use crate::calls::scope::Rule;
use crate::calls::table::Kind::{Int, Name, Project, Text};
use crate::calls::table::{a, call, opt, Call};
use crate::calls::{Context, Form, Paging};
use layerfs_protocol::provision::install::SdkValue;
use layerfs_home::outcome::Outcome;
use layerfs_store::calls::file::{read, FileRead};

/// `text` for a text file (an empty one `""`); `binary` and `size` for one
/// that is not text (the short form shows `size` only then, the long form
/// always); `next_line` while lines are left.
const FORM: Form = Form::of(
    &[],
    &["text", "next_line", "line_truncated", "binary", "size"],
    &[
        "text",
        "next_line",
        "line_truncated",
        "binary",
        "size",
        "commit",
        "root",
        "path",
        "start_line",
        "end_line",
    ],
)
.short_when(&[("size", binary)]);

/// Whether the read file is not text.
fn binary(fields: &[(String, SdkValue)], _asked: &[&str]) -> bool {
    (fields.iter()).any(|(key, value)| key == "binary" && *value == SdkValue::Bool(true))
}

/// Its table entry.
pub(crate) const CALL: Call = call(
    "file",
    "read",
    &[
        a("project", Project),
        a("branch", Name),
        a("path", Text).redact_path(),
    ],
    &[opt("at", Name), opt("line", Int), opt("limit", Int)],
    "read a text file, or a window of its lines",
    run,
)
.scoped(Rule::File(&["path"]))
.form(&FORM)
.paged(Paging::Line)
.agent();

/// Runs `file.read`.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let request = FileRead {
        project: text(args, "project").unwrap_or_default(),
        branch: text(args, "branch").unwrap_or_default(),
        at: text(args, "at"),
        path: text(args, "path").unwrap_or_default(),
        line: args.get_int("line"),
        limit: args.get_int("limit"),
    };
    let (home, events) = context.parts();
    read(home, events, &request)
}
