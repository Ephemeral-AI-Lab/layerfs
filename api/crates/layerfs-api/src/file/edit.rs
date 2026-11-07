//! `file.edit`: replace one unique text in a file; one Commit.
use crate::calls::table::Kind::{Name, Project, Text};
use crate::calls::table::{a, call, opt, Call, MESSAGE};
use crate::calls::Context;
use layerfs_bridge::control::SdkValue;
use layerfs_home::outcome::Outcome;
use layerfs_store::calls::file::{edit, FileEdit};

/// Its table entry.
pub(crate) const CALL: Call = call(
    "file",
    "edit",
    &[a("project", Project), a("branch", Name), a("path", Text)],
    &[opt("old", Text), opt("new", Text), MESSAGE],
    "replace one unique text in a file; one Commit",
    run,
);

/// Runs `file.edit`.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let text = |name: &str| args.get_text(name).map(str::to_owned);
    let request = FileEdit {
        project: text("project").unwrap_or_default(),
        branch: text("branch").unwrap_or_default(),
        path: text("path").unwrap_or_default(),
        old: text("old"),
        new: text("new"),
        message: text("message"),
    };
    let (home, events) = context.parts();
    edit(home, events, &request)
}
