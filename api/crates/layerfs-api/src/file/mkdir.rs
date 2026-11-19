//! `file.mkdir`: make a folder and its missing parents; one Commit.
use super::write::{COMMITTED, EXPECTED};
use super::{guard, text};
use crate::calls::scope::Rule;
use crate::calls::table::Kind::{Name, Project, Text};
use crate::calls::table::{a, call, opt, Call, MESSAGE};
use crate::calls::{Context, Policy};
use layerfs_bridge::control::SdkValue;
use layerfs_home::outcome::Outcome;
use layerfs_store::calls::file::{mkdir, FileMkdir};

/// Its table entry.
pub(crate) const CALL: Call = call(
    "file",
    "mkdir",
    &[
        a("project", Project),
        a("branch", Name),
        a("path", Text).redact_path(),
    ],
    &[opt("mode", Text), EXPECTED, MESSAGE],
    "make a folder and its missing parents; one Commit",
    run,
)
.scoped(Rule::File(&["path"]))
.policy(Policy::Project("project"))
.saves()
.form(&COMMITTED);

/// Runs `file.mkdir`.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let request = FileMkdir {
        project: text(args, "project").unwrap_or_default(),
        branch: text(args, "branch").unwrap_or_default(),
        path: text(args, "path").unwrap_or_default(),
        mode: text(args, "mode"),
        guard: guard(args),
    };
    let (home, events) = context.parts();
    mkdir(home, events, &request)
}
