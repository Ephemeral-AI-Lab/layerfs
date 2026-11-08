//! `file.blame`: the Commit of each line.
use crate::calls::table::Kind::{Int, Name, Project, Text};
use crate::calls::table::{a, call, opt, Call};
use crate::calls::Context;
use layerfs_bridge::control::SdkValue;
use layerfs_home::outcome::Outcome;
use layerfs_store::calls::file::{blame, FileBlame};

/// Its table entry.
pub(crate) const CALL: Call = call(
    "file",
    "blame",
    &[
        a("project", Project),
        a("branch", Name),
        a("path", Text),
        opt("commit", Name),
    ],
    &[opt("from_line", Int), opt("line_count", Int)],
    "the Commit of each line",
    run,
);

/// Runs `file.blame`.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let text = |name: &str| args.get_text(name).map(str::to_owned);
    let request = FileBlame {
        project: text("project").unwrap_or_default(),
        branch: text("branch").unwrap_or_default(),
        commit: text("commit"),
        path: text("path").unwrap_or_default(),
        from_line: args.get_int("from_line"),
        line_count: args.get_int("line_count"),
    };
    let (home, events) = context.parts();
    blame(home, events, &request)
}
