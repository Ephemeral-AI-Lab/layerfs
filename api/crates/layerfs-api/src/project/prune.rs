//! `project.prune`: remove a project's home entry that has no record.
use crate::calls::table::Kind::Project;
use crate::calls::table::{a, call, Call};
use crate::calls::{Context, Form};
use layerfs_bridge::control::SdkValue;
use layerfs_home::names::ProjectRef;
use layerfs_home::outcome::Outcome;

/// `{ok:true}`, with what was removed in the long form.
const FORM: Form = Form::of(&["ok"], &[], &["ok", "outcome", "project", "store"]);

/// Its table entry.
pub(crate) const CALL: Call = call(
    "project",
    "prune",
    &[a("project", Project)],
    &[],
    "remove a project's home entry that has no record",
    run,
)
.form(&FORM);

/// Runs `project.prune`.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let project = ProjectRef::parse(args.get_text("project").unwrap_or(""))?;
    layerfs_control::project::prune(&context.home, &project)
}
