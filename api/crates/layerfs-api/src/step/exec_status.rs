//! `step.exec_status`: one exec's status now.
use layerfs_bridge::control::SdkValue;
use layerfs_control::calls::table::Kind::Text;
use layerfs_control::calls::table::{a, call, Call};
use layerfs_control::calls::Context;
use layerfs_control::common::outcome::Outcome;

/// Its table entry.
pub(crate) const CALL: Call = call(
    "step",
    "exec_status",
    &[a("exec_id", Text)],
    &[],
    "one exec's status now",
    run,
);

/// Runs `step.exec_status`.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    layerfs_control::step::step(context, "exec_status", args)
}
