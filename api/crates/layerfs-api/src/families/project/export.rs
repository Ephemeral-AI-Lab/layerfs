//! `project.export`: export a Branch's committed tree to a folder on this
//! computer.
use crate::dispatch::table::Kind::{Name, Project, Text};
use crate::dispatch::table::{a, call, opt, Call, PROGRESS};
use crate::dispatch::{Context, Form};
use layerfs_home::names::ProjectRef;
use layerfs_home::reply::outcome::Outcome;
use layerfs_protocol::sdk::SdkValue;
use layerfs_store::data::project::export::{run as export, ProjectExport};

/// The exported tree's counts; the exported `commit` and its `root` in the
/// long form.
const FORM: Form = Form::of(
    &["file_count", "size"],
    &[],
    &[
        "file_count",
        "size",
        "dir_count",
        "symlink_count",
        "commit",
        "root",
    ],
);

/// Its table entry.
pub(crate) const CALL: Call = call(
    "project",
    "export",
    &[a("project", Project), a("destination", Text)],
    &[opt("branch", Name), opt("at", Name), PROGRESS],
    "export a Branch's committed tree to a folder on this computer",
    run,
)
.form(&FORM);

/// Runs `project.export`.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let text = |name: &str| args.get_text(name).map(str::to_owned);
    let request = ProjectExport {
        project: ProjectRef::parse(args.get_text("project").unwrap_or(""))?,
        destination: text("destination").unwrap_or_default(),
        branch: text("branch"),
        at: text("at"),
    };
    let (home, events) = context.parts();
    export(home, events, &request)
}
