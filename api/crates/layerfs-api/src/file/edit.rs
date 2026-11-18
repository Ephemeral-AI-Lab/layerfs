//! `file.edit`: replace a text in a file; one Commit.
use super::write::EXPECTED;
use super::{guard, text};
use crate::calls::scope::Rule;
use crate::calls::table::Kind::{Bool, Name, Project, Text};
use crate::calls::table::{a, call, opt, Call, MESSAGE};
use crate::calls::{Context, Form, Policy, Saves};
use layerfs_bridge::control::SdkValue;
use layerfs_home::outcome::Outcome;
use layerfs_store::calls::file::{edit, FileEdit};

/// A write's answer and how many occurrences were `replaced`: always in the
/// long form (1 for a plain edit), in the short form only past 1.
const FORM: Form = Form::of(
    &["commit"],
    &[
        "unchanged",
        "replaced",
        "merged",
        "overwritten",
        "overwritten_count",
    ],
    &[
        "commit",
        "unchanged",
        "replaced",
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
)
.short_when(&[("replaced", more_than_one)]);

/// Whether more than one occurrence was replaced.
fn more_than_one(fields: &[(String, SdkValue)]) -> bool {
    (fields.iter())
        .any(|(key, value)| key == "replaced" && matches!(value, SdkValue::Int(n) if *n > 1))
}

/// Its table entry.
pub(crate) const CALL: Call = call(
    "file",
    "edit",
    &[
        a("project", Project),
        a("branch", Name),
        a("path", Text).redact_path(),
    ],
    &[
        opt("old_string", Text).redact().required(),
        opt("new_string", Text).redact().required(),
        opt("replace_all", Bool),
        EXPECTED,
        MESSAGE,
    ],
    "replace a text in a file; one Commit",
    run,
)
.scoped(Rule::File(&["path"]))
.policy(Policy::Project("project"))
.saves(Saves::Commit)
.form(&FORM)
.agent();

/// Runs `file.edit`; the table requires `--old-string` and `--new-string`.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let request = FileEdit {
        project: text(args, "project").unwrap_or_default(),
        branch: text(args, "branch").unwrap_or_default(),
        path: text(args, "path").unwrap_or_default(),
        old_string: text(args, "old_string").unwrap_or_default(),
        new_string: text(args, "new_string").unwrap_or_default(),
        replace_all: args.get_bool("replace_all"),
        guard: guard(args),
    };
    let (home, events) = context.parts();
    edit(home, events, &request)
}
