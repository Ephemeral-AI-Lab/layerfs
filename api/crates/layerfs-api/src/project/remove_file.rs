//! `project.remove_file`: hide one absolute path from every node at or below
//! its marker frontier.
use crate::calls::table::Kind::{Project, Text};
use crate::calls::table::{a, call, Call};
use crate::calls::Context;
use layerfs_bridge::control::SdkValue;
use layerfs_home::outcome::Outcome;
use layerfs_store::calls::project::{remove_file, ProjectRemoveFile};

/// Its table entry.
pub(crate) const CALL: Call = call(
    "project",
    "remove_file",
    &[a("project", Project), a("path", Text).redact_path()],
    &[],
    "hide one absolute path from every node at or below its marker frontier",
    run,
);

/// Runs `project.remove_file`.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let text = |name: &str| args.get_text(name).map(str::to_owned);
    let request = ProjectRemoveFile {
        project: text("project").unwrap_or_default(),
        path: text("path").unwrap_or_default(),
    };
    let (home, events) = context.parts();
    remove_file(home, events, &request)
}
