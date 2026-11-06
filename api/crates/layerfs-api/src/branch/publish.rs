//! `branch.publish`: a Branch's head or Commit as main's next Layer.
use crate::calls::table::Kind::{Name, Project};
use crate::calls::table::{a, call, opt, Call};
use crate::calls::Context;
use layerfs_bridge::control::SdkValue;
use layerfs_home::outcome::Outcome;

/// Its table entry.
pub(crate) const CALL: Call = call(
    "branch",
    "publish",
    &[
        a("project", Project),
        a("branch", Name),
        opt("commit", Name),
    ],
    &[],
    "a Branch's head or Commit as main's next Layer",
    run,
);

/// Runs `branch.publish`.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let (home, events) = context.parts();
    layerfs_store::calls::branch_publish(home, events, args)
}
