//! `file.blame`: the Commit that last changed each line of a text file.
use super::text;
use crate::dispatch::guard::scope::Rule;
use crate::dispatch::table::Kind::{Int, Name, Project, Text};
use crate::dispatch::table::{a, call, opt, Call};
use crate::dispatch::{Context, Form, Paging, Part};
use layerfs_home::outcome::Outcome;
use layerfs_protocol::sdk::SdkValue;
use layerfs_store::calls::file::{blame, FileBlame};

/// A run of lines with one owner; one without `commit` has none found.
const RANGE: Form = Form::of(&["start", "end"], &["commit"], &["start", "end", "commit"]);
/// Each owner once.
const OWNER: Form = Form::of(
    &["commit"],
    &["message"],
    &["commit", "message", "created_at"],
);
/// `partial` when the history budget ran out or a line was cut.
const FORM: Form = Form::of(
    &["ranges", "commits"],
    &["next_line", "partial"],
    &[
        "ranges",
        "commits",
        "next_line",
        "partial",
        "commit",
        "root",
        "path",
    ],
)
.parts(&[
    ("ranges", Part::Rows(&RANGE)),
    ("commits", Part::Rows(&OWNER)),
]);

/// Its table entry.
pub(crate) const CALL: Call = call(
    "file",
    "blame",
    &[
        a("project", Project),
        a("branch", Name),
        a("path", Text).redact_path(),
    ],
    &[opt("at", Name), opt("line", Int), opt("limit", Int)],
    "the Commit that last changed each line of a text file",
    run,
)
.scoped(Rule::File(&["path"]))
.form(&FORM)
.paged(Paging::Line);

/// Runs `file.blame`.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let request = FileBlame {
        project: text(args, "project").unwrap_or_default(),
        branch: text(args, "branch").unwrap_or_default(),
        at: text(args, "at"),
        path: text(args, "path").unwrap_or_default(),
        line: args.get_int("line"),
        limit: args.get_int("limit"),
    };
    let (home, events) = context.parts();
    blame(home, events, &request)
}
