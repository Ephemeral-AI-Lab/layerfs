//! `branch.merge`: merge two Branches into a new one.
use layerfs_bridge::control::SdkValue;
use layerfs_control::calls::table::Kind::{Json, Name, Project};
use layerfs_control::calls::table::{a, call, opt, Call, MESSAGE};
use layerfs_control::calls::Context;
use layerfs_control::common::outcome::Outcome;

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
    layerfs_control::store::calls::run(context, "branch", "merge", args)
}
