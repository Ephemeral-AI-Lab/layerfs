//! `file.read`: read a file.
use crate::calls::table::Kind::{Int, Name, Project, Text};
use crate::calls::table::{a, call, opt, Call};
use crate::calls::Context;
use layerfs_bridge::control::SdkValue;
use layerfs_home::outcome::Outcome;
use layerfs_store::calls::file::{read, FileRead};

/// Its table entry.
pub(crate) const CALL: Call = call(
    "file",
    "read",
    &[
        a("project", Project),
        a("branch", Name),
        a("path", Text),
        opt("commit", Name),
    ],
    &[opt("start_line", Int), opt("end_line", Int)],
    "read a file",
    run,
);

/// Runs `file.read`.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let text = |name: &str| args.get_text(name).map(str::to_owned);
    let request = FileRead {
        project: text("project").unwrap_or_default(),
        branch: text("branch").unwrap_or_default(),
        commit: text("commit"),
        path: text("path").unwrap_or_default(),
        start_line: args.get_int("start_line"),
        end_line: args.get_int("end_line"),
    };
    let (home, events) = context.parts();
    read(home, events, &request)
}
