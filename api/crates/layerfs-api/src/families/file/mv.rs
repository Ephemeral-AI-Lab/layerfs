//! `file.mv`: move a path; one Commit.
use super::write::{COMMITTED, EXPECTED};
use super::{guard, text};
use crate::dispatch::guard::scope::Rule;
use crate::dispatch::table::Kind::{Name, Project, Text};
use crate::dispatch::table::{a, call, Call, MESSAGE};
use crate::dispatch::{Context, Policy};
use layerfs_home::outcome::Outcome;
use layerfs_protocol::sdk::SdkValue;
use layerfs_store::calls::file::{mv, FileMv};

/// Its table entry.
pub(crate) const CALL: Call = call(
    "file",
    "mv",
    &[
        a("project", Project),
        a("branch", Name),
        a("from", Text).redact_path(),
        a("to", Text).redact_path(),
    ],
    &[EXPECTED, MESSAGE],
    "move a path; one Commit",
    run,
)
.scoped(Rule::File(&["from", "to"]))
.policy(Policy::Project("project"))
.saves()
.form(&COMMITTED)
.agent();

/// Runs `file.mv`.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let request = FileMv {
        project: text(args, "project").unwrap_or_default(),
        branch: text(args, "branch").unwrap_or_default(),
        from: text(args, "from").unwrap_or_default(),
        to: text(args, "to").unwrap_or_default(),
        guard: guard(args),
    };
    let (home, events) = context.parts();
    mv(home, events, &request)
}
