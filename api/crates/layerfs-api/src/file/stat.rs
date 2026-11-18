//! `file.stat`: a path's type, size and mode.
use super::text;
use crate::calls::scope::Rule;
use crate::calls::table::Kind::{Name, Project, Text};
use crate::calls::table::{a, call, opt, Call};
use crate::calls::{Context, Form};
use layerfs_bridge::control::SdkValue;
use layerfs_home::outcome::Outcome;
use layerfs_store::calls::file::{stat, FileStat};

/// `type` is file, dir or symlink; `mode` octal text, `0644`.
const FORM: Form = Form::of(
    &["type", "size", "mode"],
    &[],
    &[
        "type",
        "size",
        "mode",
        "commit",
        "root",
        "path",
        "inode",
        "modified_at",
    ],
);

/// Its table entry.
pub(crate) const CALL: Call = call(
    "file",
    "stat",
    &[
        a("project", Project),
        a("branch", Name),
        a("path", Text).redact_path(),
    ],
    &[opt("at", Name)],
    "a path's type, size and mode",
    run,
)
.scoped(Rule::File(&["path"]))
.form(&FORM);

/// Runs `file.stat`.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let request = FileStat {
        project: text(args, "project").unwrap_or_default(),
        branch: text(args, "branch").unwrap_or_default(),
        at: text(args, "at"),
        path: text(args, "path").unwrap_or_default(),
    };
    let (home, events) = context.parts();
    stat(home, events, &request)
}
