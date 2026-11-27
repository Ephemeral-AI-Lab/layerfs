//! `project.init`: import a folder on this computer as a new project into
//! an existing Store.
use crate::dispatch::table::Kind::{Int, Project, Text};
use crate::dispatch::table::{a, call, opt, Call, PROGRESS};
use crate::dispatch::{Context, Form, Policy};
use layerfs_home::names::ProjectRef;
use layerfs_home::reply::outcome::Outcome;
use layerfs_protocol::sdk::SdkValue;
use layerfs_store::data::project::init::run::{init, ProjectInit};

/// `commit` is main's first Layer, the full id.
const FORM: Form = Form::of(
    &["commit"],
    &[],
    &[
        "commit",
        "outcome",
        "project",
        "store",
        "engine",
        "machine",
        "root",
        "created_at",
    ],
);

/// Its table entry.
pub(crate) const CALL: Call = call(
    "project",
    "init",
    &[a("project", Project), a("folder", Text)],
    &[opt("timeout", Int), PROGRESS],
    "import a folder on this computer as a new project into an existing Store",
    run,
)
.policy(Policy::Project("project"))
.form(&FORM);

/// Runs `project.init`.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let request = ProjectInit {
        project: ProjectRef::parse(args.get_text("project").unwrap_or(""))?,
        folder: args.get_text("folder").unwrap_or("").to_owned(),
        timeout: args.get_int("timeout"),
    };
    let (home, events) = context.parts();
    init(home, events, &request)
}
