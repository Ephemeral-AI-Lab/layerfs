//! `step.exec_async`: start one command and answer its durable exec id.
use layerfs_bridge::control::SdkValue;
use layerfs_control::calls::table::Kind::{Int, Text};
use layerfs_control::calls::table::{a, call, opt, Call};
use layerfs_control::calls::Context;
use layerfs_control::common::outcome::Outcome;

/// Its table entry.
pub(crate) const CALL: Call = call(
    "step",
    "exec_async",
    &[a("key", Text), a("command", Text)],
    &[opt("timeout_ms", Int)],
    "start one command asynchronously and answer its exec id",
    run,
);

/// Runs `step.exec_async`.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    layerfs_control::step::step(context, "exec_async", args)
}
