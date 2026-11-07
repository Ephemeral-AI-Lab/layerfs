//! `branch.merge`: merge two Branches into a new one.
use crate::calls::table::Kind::{Json, Name, Project};
use crate::calls::table::{a, call, opt, Call, MESSAGE};
use crate::calls::Context;
use layerfs_bridge::control::SdkValue;
use layerfs_home::outcome::Outcome;
use layerfs_store::calls::branch::{merge, BranchMerge};

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
    let text = |name: &str| args.get_text(name).map(str::to_owned);
    let request = BranchMerge {
        project: text("project").unwrap_or_default(),
        name: text("name").unwrap_or_default(),
        a: text("a").unwrap_or_default(),
        b: text("b").unwrap_or_default(),
        message: text("message"),
        units: args.get("units").cloned(),
    };
    let (home, events) = context.parts();
    merge(home, events, &request)
}
