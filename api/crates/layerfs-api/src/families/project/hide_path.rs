//! `project.hide_path` (was `project.remove_file`): hide one absolute path
//! from the project's whole history. It writes a marker, not a Commit, and
//! frees no bytes.
use crate::dispatch::table::Kind::{Project, Text};
use crate::dispatch::table::{a, call, Call};
use crate::dispatch::{Context, Form};
use layerfs_home::outcome::Outcome;
use layerfs_protocol::sdk::SdkValue;
use layerfs_store::calls::project::{hide_path, ProjectHidePath};

/// `{ok:true}`, or `unchanged` for a path already hidden.
const FORM: Form = Form::of(
    &[],
    &["ok", "unchanged"],
    &[
        "ok",
        "unchanged",
        "outcome",
        "path",
        "frontier_serial",
        "hidden_at",
    ],
);

/// Its table entry.
pub(crate) const CALL: Call = call(
    "project",
    "hide_path",
    &[a("project", Project), a("path", Text).redact_path()],
    &[],
    "hide one absolute path from the project's whole history; frees no bytes",
    run,
)
.form(&FORM);

/// Runs `project.hide_path`.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let text = |name: &str| args.get_text(name).map(str::to_owned);
    let request = ProjectHidePath {
        project: text("project").unwrap_or_default(),
        path: text("path").unwrap_or_default(),
    };
    let (home, events) = context.parts();
    hide_path(home, events, &request)
}
