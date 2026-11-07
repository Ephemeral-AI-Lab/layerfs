//! `file.rm`: remove a path; one Commit.
use crate::calls::table::Kind::{Name, Project, Text};
use crate::calls::table::{a, call, Call, MESSAGE};
use crate::calls::Context;
use layerfs_bridge::control::SdkValue;
use layerfs_home::outcome::Outcome;
use layerfs_store::calls::file::{rm, FileRm};

/// Its table entry.
pub(crate) const CALL: Call = call(
    "file",
    "rm",
    &[a("project", Project), a("branch", Name), a("path", Text)],
    &[MESSAGE],
    "remove a path; one Commit",
    run,
);

/// Runs `file.rm`.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let text = |name: &str| args.get_text(name).map(str::to_owned);
    let request = FileRm {
        project: text("project").unwrap_or_default(),
        branch: text("branch").unwrap_or_default(),
        path: text("path").unwrap_or_default(),
        message: text("message"),
    };
    let (home, events) = context.parts();
    rm(home, events, &request)
}
