//! `file.write`: write a file; one Commit.
use crate::calls::table::Kind::{Content, Int, Name, Project, Text};
use crate::calls::table::{a, call, opt, Call, MESSAGE};
use crate::calls::Context;
use layerfs_bridge::control::SdkValue;
use layerfs_home::outcome::Outcome;
use layerfs_store::calls::file::{write, FileWrite};
use layerfs_store::calls::Content as Body;

/// Its table entry.
pub(crate) const CALL: Call = call(
    "file",
    "write",
    &[
        a("project", Project),
        a("branch", Name),
        a("path", Text),
        a("content", Content),
    ],
    &[MESSAGE, opt("mode", Int)],
    "write a file; one Commit",
    run,
);

/// Runs `file.write`.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let text = |name: &str| args.get_text(name).map(str::to_owned);
    let request = FileWrite {
        project: text("project").unwrap_or_default(),
        branch: text("branch").unwrap_or_default(),
        path: text("path").unwrap_or_default(),
        content: match args.get("content") {
            Some(SdkValue::Bytes(bytes)) => Body::Bytes(bytes.clone()),
            _ => Body::Text(text("content").unwrap_or_default()),
        },
        message: text("message"),
        mode: args.get_int("mode"),
    };
    let (home, events) = context.parts();
    write(home, events, &request)
}
