//! `file.stat`: a path's kind, size and mode.
use crate::calls::table::Kind::{Name, Project, Text};
use crate::calls::table::{a, call, opt, Call};
use crate::calls::Context;
use layerfs_bridge::control::SdkValue;
use layerfs_home::outcome::Outcome;
use layerfs_store::calls::file::{stat, FileStat};

/// Its table entry.
pub(crate) const CALL: Call = call(
    "file",
    "stat",
    &[
        a("project", Project),
        a("branch", Name),
        a("path", Text),
        opt("commit", Name),
    ],
    &[],
    "a path's kind, size and mode",
    run,
);

/// Runs `file.stat`.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let text = |name: &str| args.get_text(name).map(str::to_owned);
    let request = FileStat {
        project: text("project").unwrap_or_default(),
        branch: text("branch").unwrap_or_default(),
        commit: text("commit"),
        path: text("path").unwrap_or_default(),
    };
    let (home, events) = context.parts();
    stat(home, events, &request)
}
