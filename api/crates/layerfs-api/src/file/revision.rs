//! `file.revision`: saved changes to one literal path.
use crate::calls::table::Kind::{Int, Name, Project, Text};
use crate::calls::table::{a, call, opt, Call};
use crate::calls::Context;
use layerfs_bridge::control::SdkValue;
use layerfs_home::outcome::Outcome;
use layerfs_store::calls::file::{revision, FileRevision};
use layerfs_store::calls::Page;

pub(crate) const CALL: Call = call(
    "file",
    "revision",
    &[a("project", Project), a("branch", Name), a("path", Text)],
    &[
        opt("at_commit", Name),
        opt("traversal", Text),
        opt("page", Int),
        opt("page_size", Int),
        opt("continuation", Text),
    ],
    "saved changes to a literal path",
    run,
);

fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let text = |name: &str| args.get_text(name).map(str::to_owned);
    let request = FileRevision {
        project: text("project").unwrap_or_default(),
        branch: text("branch").unwrap_or_default(),
        path: text("path").unwrap_or_default(),
        at_commit: text("at_commit"),
        traversal: text("traversal"),
        continuation: text("continuation"),
        page: Page {
            size: args.get_int("page_size"),
            number: args.get_int("page"),
            pos: None,
        },
    };
    let (home, events) = context.parts();
    revision(home, events, &request)
}
