//! `project.get`: one project: its Store, state and main's head.
use crate::calls::table::Kind::Project;
use crate::calls::table::{a, call, Call};
use crate::calls::{Context, Form};
use layerfs_protocol::provision::install::SdkValue;
use layerfs_home::names::ProjectRef;
use layerfs_home::outcome::Outcome;

/// Its Store and state always, with the state's companion; `commit` is
/// main's head.
const FORM: Form = Form::of(
    &["store", "state"],
    &["commit", "op", "blocked_by", "reason", "fix"],
    &[
        "store",
        "state",
        "commit",
        "op",
        "blocked_by",
        "reason",
        "fix",
        "project",
        "engine",
        "machine",
        "created_at",
    ],
);

/// Its table entry.
pub(crate) const CALL: Call = call(
    "project",
    "get",
    &[a("project", Project)],
    &[],
    "one project: its Store, state and main's head",
    run,
)
.form(&FORM);

/// Runs `project.get`.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let project = ProjectRef::parse(args.get_text("project").unwrap_or(""))?;
    layerfs_store::data::project::with_users::get(&context.home, &project)
}
