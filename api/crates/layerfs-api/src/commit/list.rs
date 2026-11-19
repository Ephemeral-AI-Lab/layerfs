//! `commit.list`: a Branch's Commits, newest first.
use crate::calls::scope::Rule;
use crate::calls::table::Kind::{Int, Name, Project, Text};
use crate::calls::table::{a, call, opt, Call};
use crate::calls::{Context, Form, Paging, Part};
use layerfs_bridge::control::SdkValue;
use layerfs_home::outcome::Outcome;
use layerfs_store::calls::commit::{list, CommitList};

/// One Commit; on main, one publication with the Branch it came from.
const ROW: Form = Form::of(
    &["commit"],
    &["message", "message_truncated", "source_branch"],
    &[
        "commit",
        "message",
        "message_truncated",
        "source_branch",
        "branch",
        "parent",
        "second_parent",
        "base",
        "root",
        "via",
        "created_at",
        "merged_from",
        "source_commit",
        "size",
        "new_canonical_bytes",
        "new_stored_bytes",
        "new_metadata_bytes",
    ],
);
const FORM: Form = Form::of(&["items"], &["next_cursor"], &["items", "next_cursor"])
    .parts(&[("items", Part::Rows(&ROW))]);

/// Its table entry.
pub(crate) const CALL: Call = call(
    "commit",
    "list",
    &[a("project", Project), a("branch", Name)],
    &[
        opt("at", Name),
        opt("limit", Int),
        opt("cursor", Text).redact(),
    ],
    "a Branch's Commits, newest first",
    run,
)
.scoped(Rule::History)
.form(&FORM)
.paged(Paging::Cursor)
.agent();

/// Runs `commit.list`.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let text = |name: &str| args.get_text(name).map(str::to_owned);
    let request = CommitList {
        project: text("project").unwrap_or_default(),
        branch: text("branch").unwrap_or_default(),
        at: text("at"),
        limit: args.get_int("limit"),
        cursor: text("cursor"),
    };
    let (home, events) = context.parts();
    list(home, events, &request)
}
