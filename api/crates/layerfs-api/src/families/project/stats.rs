//! `project.stats`: entry counts and size of the tree at a Branch's head or
//! a Commit.
use crate::dispatch::table::Kind::{Name, Project};
use crate::dispatch::table::{a, call, opt, Call};
use crate::dispatch::{Context, Form};
use layerfs_home::outcome::Outcome;
use layerfs_protocol::sdk::SdkValue;
use layerfs_store::calls::project::{stats, ProjectStats};

/// The counts and size; `partial` when the count stopped at its work
/// bound; the counted `commit` and its `root` in the long form.
const FORM: Form = Form::of(
    &["file_count", "dir_count", "symlink_count", "size"],
    &["partial"],
    &[
        "file_count",
        "dir_count",
        "symlink_count",
        "size",
        "partial",
        "commit",
        "root",
    ],
);

/// Its table entry.
pub(crate) const CALL: Call = call(
    "project",
    "stats",
    &[a("project", Project)],
    &[opt("branch", Name), opt("at", Name)],
    "entry counts and size of the tree at a Branch's head or a Commit",
    run,
)
.form(&FORM);

/// Runs `project.stats`.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let request = ProjectStats {
        project: args.get_text("project").unwrap_or("").to_owned(),
        branch: args.get_text("branch").unwrap_or("main").to_owned(),
        at: args.get_text("at").map(str::to_owned),
    };
    let (home, events) = context.parts();
    stats(home, events, &request)
}
