//! `commit.diff`: the paths that differ between two Commits or Branches.
use crate::dispatch::guard::scope::Rule;
use crate::dispatch::table::Kind::{Int, Name, Project, Text};
use crate::dispatch::table::{a, call, opt, Call};
use crate::dispatch::{Context, Form, Paging, Part, Section};
use layerfs_home::reply::outcome::Outcome;
use layerfs_protocol::sdk::SdkValue;
use layerfs_store::data::commit::{diff, CommitDiff};

/// One path that differs; `change` is added, modified or removed.
const ROW: Form = Form::of(
    &["path", "change"],
    &[],
    &["path", "change", "type", "content", "metadata", "identity"],
);
/// One line of a `--with patch` hunk list: `op` and `text` in both forms,
/// a blank line's empty `text` too.
const HUNK: Form = Form::of(&["op", "text"], &[], &["op", "text"]);
/// What `--with patch` adds to an item.
const PATCH: Form = Form::of(
    &[],
    &["hunks", "hunks_omitted"],
    &["hunks", "hunks_omitted"],
)
.parts(&[("hunks", Part::Rows(&HUNK))]);
const FORM: Form = Form::of(
    &["items"],
    &["next_cursor"],
    &["items", "next_cursor", "from", "to", "from_root", "to_root"],
)
.parts(&[("items", Part::Rows(&ROW))]);

/// Its table entry.
pub(crate) const CALL: Call = call(
    "commit",
    "diff",
    &[a("project", Project), a("ref", Name)],
    &[
        opt("from", Name),
        opt("path", Text).redact_path(),
        opt("limit", Int),
        opt("cursor", Text).redact(),
    ],
    "the paths that differ between two Commits or Branches",
    run,
)
.scoped(Rule::History)
.form(&FORM)
.with(&[Section::item("patch", &["hunks", "hunks_omitted"]).shaped(&PATCH)])
.paged(Paging::Cursor)
.agent();

/// Runs `commit.diff`: each text file's lines are read only when
/// `--with patch` asks for them.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let text = |name: &str| args.get_text(name).map(str::to_owned);
    let request = CommitDiff {
        project: text("project").unwrap_or_default(),
        reference: text("ref").unwrap_or_default(),
        from: text("from"),
        path: text("path"),
        patch: context.with("patch"),
        limit: args.get_int("limit"),
        cursor: text("cursor"),
    };
    let (home, events) = context.parts();
    diff(home, events, &request)
}
