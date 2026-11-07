//! `file.grep`: lines under `path` matching a regular expression.
use crate::calls::table::Kind::{Name, Project, Text};
use crate::calls::table::{a, call, opt, Call, PAGE};
use crate::calls::Context;
use layerfs_bridge::control::SdkValue;
use layerfs_home::outcome::Outcome;
use layerfs_store::calls::file::{grep, FileGrep};
use layerfs_store::calls::Page;

/// Its table entry.
pub(crate) const CALL: Call = call(
    "file",
    "grep",
    &[
        a("project", Project),
        a("branch", Name),
        a("path", Text),
        a("pattern", Text),
        opt("commit", Name),
    ],
    &PAGE,
    "lines under `path` matching a regular expression",
    run,
);

/// Runs `file.grep`.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let text = |name: &str| args.get_text(name).map(str::to_owned);
    let request = FileGrep {
        project: text("project").unwrap_or_default(),
        branch: text("branch").unwrap_or_default(),
        commit: text("commit"),
        path: text("path").unwrap_or_default(),
        pattern: text("pattern").unwrap_or_default(),
        page: Page {
            size: args.get_int("page_size"),
            pos: args.get_int("page_pos"),
        },
    };
    let (home, events) = context.parts();
    grep(home, events, &request)
}
