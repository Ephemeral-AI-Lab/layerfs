//! `step.cancel`: cancel one exec by its durable id.
use layerfs_bridge::control::SdkValue;
use layerfs_control::calls::table::Kind::Text;
use layerfs_control::calls::table::{a, call, Call};
use layerfs_control::calls::Context;
use layerfs_control::common::outcome::Outcome;

/// Its table entry.
pub(crate) const CALL: Call = call(
    "step",
    "cancel",
    &[a("exec_id", Text)],
    &[],
    "cancel one exec by its durable id",
    run,
);

/// Runs `step.cancel`.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    layerfs_control::step::step(context, "cancel", args)
}
