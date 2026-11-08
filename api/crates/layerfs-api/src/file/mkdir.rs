//! `file.mkdir`: make a folder; one Commit.
use crate::calls::table::Kind::{Int, Name, Project, Text};
use crate::calls::table::{a, call, opt, Call, MESSAGE};
use crate::calls::Context;
use layerfs_bridge::control::SdkValue;
use layerfs_home::outcome::Outcome;
use layerfs_store::calls::file::{mkdir, FileMkdir};

/// Its table entry.
pub(crate) const CALL: Call = call(
    "file",
    "mkdir",
    &[a("project", Project), a("branch", Name), a("path", Text)],
    &[MESSAGE, opt("mode", Int)],
    "make a folder; one Commit",
    run,
);

/// Runs `file.mkdir`.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let text = |name: &str| args.get_text(name).map(str::to_owned);
    let request = FileMkdir {
        project: text("project").unwrap_or_default(),
        branch: text("branch").unwrap_or_default(),
        path: text("path").unwrap_or_default(),
        message: text("message"),
        mode: args.get_int("mode"),
    };
    let (home, events) = context.parts();
    mkdir(home, events, &request)
}
