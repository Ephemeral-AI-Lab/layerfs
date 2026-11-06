//! `branch.merge`: merge two Branches into a new one.
use crate::calls::table::Kind::{Json, Name, Project};
use crate::calls::table::{a, call, opt, Call, MESSAGE};
use crate::calls::Context;
use layerfs_bridge::control::SdkValue;
use layerfs_home::outcome::Outcome;

/// Its table entry.
pub(crate) const CALL: Call = call(
    "branch",
    "merge",
    &[
        a("project", Project),
        a("name", Name),
        a("a", Name),
        a("b", Name),
    ],
    &[MESSAGE, opt("units", Json)],
    "merge two Branches into a new one",
    run,
);

/// Runs `branch.merge`.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let (home, events) = context.parts();
    layerfs_store::calls::branch_merge(home, events, args)
}
