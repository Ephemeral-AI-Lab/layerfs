//! `file.glob`: paths under `path` matching a glob.
use crate::calls::table::Kind::{Int, Name, Project, Text};
use crate::calls::table::{a, call, opt, Call};
use crate::calls::Context;
use layerfs_bridge::control::SdkValue;
use layerfs_home::outcome::Outcome;
use layerfs_store::calls::file::{glob, FileGlob};
use layerfs_store::calls::Page;

/// Its table entry.
pub(crate) const CALL: Call = call(
    "file",
    "glob",
    &[
        a("project", Project),
        a("branch", Name),
        a("path", Text),
        a("pattern", Text),
        opt("commit", Name),
    ],
    &[
        opt("page_size", Int),
        opt("page", Int),
        opt("page_pos", Int),
    ],
    "paths under `path` matching a glob",
    run,
);

/// Runs `file.glob`.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let text = |name: &str| args.get_text(name).map(str::to_owned);
    let request = FileGlob {
        project: text("project").unwrap_or_default(),
        branch: text("branch").unwrap_or_default(),
        commit: text("commit"),
        path: text("path").unwrap_or_default(),
        pattern: text("pattern").unwrap_or_default(),
        page: Page {
            size: args.get_int("page_size"),
            pos: args.get_int("page_pos"),
            number: args.get_int("page"),
        },
    };
    let (home, events) = context.parts();
    glob(home, events, &request)
}
