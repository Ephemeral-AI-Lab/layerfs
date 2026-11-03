//! `branch.publish`: a Branch's head or Commit as main's next Layer.
use layerfs_bridge::control::SdkValue;
use layerfs_control::calls::table::Kind::{Name, Project};
use layerfs_control::calls::table::{a, call, opt, Call};
use layerfs_control::calls::Context;
use layerfs_control::common::outcome::Outcome;

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
    layerfs_control::store::calls::run(context, "branch", "publish", args)
}
