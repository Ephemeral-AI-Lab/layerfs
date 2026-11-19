//! `file.glob`: paths under a folder that match a glob.
use super::text;
use crate::calls::scope::Rule;
use crate::calls::table::Kind::{Int, Name, Project, Text};
use crate::calls::table::{a, call, opt, Call};
use crate::calls::{Context, Form, Paging, Part};
use layerfs_bridge::control::SdkValue;
use layerfs_home::outcome::Outcome;
use layerfs_store::calls::file::{glob, FileGlob};

/// Its rows are paths.
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
.parts(&[("items", Part::Scalars("path"))]);

/// Its table entry.
pub(crate) const CALL: Call = call(
    "file",
    "glob",
    &[
        a("project", Project),
        a("branch", Name),
        a("pattern", Text).redact(),
        opt("path", Text).redact_path(),
    ],
    &[
        opt("exclude", Text).redact(),
        opt("at", Name),
        opt("limit", Int),
        opt("cursor", Text).redact(),
    ],
    "paths under a folder that match a glob",
    run,
)
.scoped(Rule::File(&["path"]))
.form(&FORM)
.paged(Paging::Cursor)
.agent();

/// Runs `file.glob`.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let request = FileGlob {
        project: text(args, "project").unwrap_or_default(),
        branch: text(args, "branch").unwrap_or_default(),
        pattern: text(args, "pattern").unwrap_or_default(),
        path: text(args, "path"),
        exclude: text(args, "exclude"),
        at: text(args, "at"),
        limit: args.get_int("limit"),
        cursor: text(args, "cursor"),
        ..FileGlob::default()
    };
    let (home, events) = context.parts();
    glob(home, events, &request)
}
