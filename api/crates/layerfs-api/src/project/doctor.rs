//! `project.doctor`: one project's record, and its record against the
//! Store's own list (a project the Store does not list is a notice with
//! its fix). `--full`, the walk of everything its Branches reach, is not
//! built yet.
use crate::calls::table::Kind::{Bool, Project};
use crate::calls::table::{a, call, opt, Call, PROGRESS};
use crate::calls::Context;
use crate::doctor::answer::{self, FORM};
use layerfs_bridge::control::SdkValue;
use layerfs_control::doctor::{Kind, Options};
use layerfs_home::outcome::{refused, Outcome};

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
    if args.get("full") == Some(&SdkValue::Bool(true)) {
        return Err(refused(
            "Unsupported",
            "project doctor --full (reading everything the project's Branches reach) is not built yet; `layerfs project doctor <project>` checks its record",
        ));
    }
    let project = args.get_text("project").unwrap_or("");
    answer::run(context, Kind::Project, project, Options::default())
}
