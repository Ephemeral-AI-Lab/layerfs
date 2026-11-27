//! `file.rm`: remove a path; one Commit.
use super::write::{COMMITTED, EXPECTED};
use super::{guard, text};
use crate::calls::scope::Rule;
use crate::calls::table::Kind::{Bool, Name, Project, Text};
use crate::calls::table::{a, call, opt, Call, MESSAGE};
use crate::calls::{Context, Policy};
use layerfs_home::outcome::Outcome;
use layerfs_protocol::sdk::SdkValue;
use layerfs_store::calls::file::{rm, FileRm};

/// Its table entry.
pub(crate) const CALL: Call = call(
    "file",
    "rm",
    &[
        a("project", Project),
        a("branch", Name),
        a("path", Text).redact_path(),
    ],
    &[opt("recursive", Bool), EXPECTED, MESSAGE],
    "remove a path; one Commit",
    run,
)
.scoped(Rule::File(&["path"]))
.policy(Policy::Project("project"))
.saves()
.form(&COMMITTED)
.agent();

/// Runs `file.rm`.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let request = FileRm {
        project: text(args, "project").unwrap_or_default(),
        branch: text(args, "branch").unwrap_or_default(),
        path: text(args, "path").unwrap_or_default(),
        recursive: args.get_bool("recursive"),
        guard: guard(args),
    };
    let (home, events) = context.parts();
    rm(home, events, &request)
}
