//! `step.exec_wait`: wait for one exec to reach a terminal status.
use layerfs_bridge::control::SdkValue;
use layerfs_control::calls::table::Kind::{Int, Text};
use layerfs_control::calls::table::{a, call, opt, Call};
use layerfs_control::calls::Context;
use layerfs_control::common::outcome::Outcome;

/// Its table entry.
pub(crate) const CALL: Call = call(
    "step",
    "exec_wait",
    &[a("exec_id", Text)],
    &[opt("wait_ms", Int)],
    "wait for one exec to reach a terminal status",
    run,
);

/// Runs `step.exec_wait`.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    layerfs_control::step::step(context, "exec_wait", args)
}
