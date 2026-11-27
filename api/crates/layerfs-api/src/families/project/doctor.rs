//! `project.doctor`: one project's record, and its record against the
//! Store's own list (a project the Store does not list is a notice with
//! its fix); `--full` also reads and checks everything main's head and
//! each Branch's head reach (`project.data`), sending `data` progress
//! events while it reads.
use crate::dispatch::table::Kind::{Bool, Project};
use crate::dispatch::table::{a, call, opt, Call, PROGRESS};
use crate::dispatch::Context;
use crate::doctor::{self, FORM};
use layerfs_home::reply::outcome::Outcome;
use layerfs_inspect::{Kind, Options};
use layerfs_protocol::sdk::SdkValue;

/// Its table entry.
pub(crate) const CALL: Call = call(
    "project",
    "doctor",
    &[a("project", Project)],
    &[opt("full", Bool), PROGRESS],
    "check one project's record; with --full, also read and check its data",
    run,
)
.unopened()
.form(&FORM);

fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let project = args.get_text("project").unwrap_or("");
    let options = Options {
        full: args.get("full") == Some(&SdkValue::Bool(true)),
        ..Options::default()
    };
    doctor::run(context, Kind::Project, project, options)
}
