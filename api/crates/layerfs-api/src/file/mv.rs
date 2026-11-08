//! `file.mv`: move a path; one Commit.
use crate::calls::table::Kind::{Name, Project, Text};
use crate::calls::table::{a, call, Call, MESSAGE};
use crate::calls::Context;
use layerfs_bridge::control::SdkValue;
use layerfs_home::outcome::Outcome;
use layerfs_store::calls::file::{mv, FileMv};

/// Its table entry.
pub(crate) const CALL: Call = call(
    "file",
    "mv",
    &[
        a("project", Project),
        a("branch", Name),
        a("from_path", Text),
        a("to_path", Text),
    ],
    &[MESSAGE],
    "move a path; one Commit",
    run,
);

/// Runs `file.mv`.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let text = |name: &str| args.get_text(name).map(str::to_owned);
    let request = FileMv {
        project: text("project").unwrap_or_default(),
        branch: text("branch").unwrap_or_default(),
        from_path: text("from_path").unwrap_or_default(),
        to_path: text("to_path").unwrap_or_default(),
        message: text("message"),
    };
    let (home, events) = context.parts();
    mv(home, events, &request)
}
