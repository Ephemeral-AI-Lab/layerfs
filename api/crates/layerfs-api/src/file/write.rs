//! `file.write`: write a file; one Commit.
use super::{guard, text};
use crate::calls::scope::Rule;
use crate::calls::table::Kind::{Content, Name, Project, Text};
use crate::calls::table::{a, call, opt, Call, MESSAGE};
use crate::calls::{Context, Form, Policy};
use layerfs_protocol::provision::install::SdkValue;
use layerfs_home::outcome::Outcome;
use layerfs_store::calls::file::{write, FileWrite};
use layerfs_store::calls::Content as Body;

/// The answer of every write but `file.edit`: `unchanged` when no Commit
/// was made (`commit` is the head), `merged` when the Branch had moved and
/// the change was merged, and the paths `.lww` rules overwrote.
pub(super) const COMMITTED: Form = Form::of(
    &["commit"],
    &["unchanged", "merged", "overwritten", "overwritten_count"],
    &[
        "commit",
        "unchanged",
        "merged",
        "overwritten",
        "overwritten_count",
        "outcome",
        "branch",
        "parent",
        "root",
        "message",
        "created_at",
    ],
);

/// A write's head guard.
pub(super) const EXPECTED: crate::calls::table::Arg = opt("expected", Name);

/// Its table entry.
pub(crate) const CALL: Call = call(
    "file",
    "write",
    &[
        a("project", Project),
        a("branch", Name),
        a("path", Text).redact_path(),
        a("content", Content),
    ],
    &[opt("mode", Text), EXPECTED, MESSAGE],
    "write a file; one Commit",
    run,
)
.scoped(Rule::File(&["path"]))
.policy(Policy::Project("project"))
.saves()
.form(&COMMITTED)
.agent();

/// Runs `file.write`.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let request = FileWrite {
        project: text(args, "project").unwrap_or_default(),
        branch: text(args, "branch").unwrap_or_default(),
        path: text(args, "path").unwrap_or_default(),
        content: match args.get("content") {
            Some(SdkValue::Bytes(bytes)) => Body::Bytes(bytes.clone()),
            _ => Body::Text(text(args, "content").unwrap_or_default()),
        },
        mode: text(args, "mode"),
        guard: guard(args),
    };
    let (home, events) = context.parts();
    write(home, events, &request)
}
