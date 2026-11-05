//! `sandbox.close`: stop a Sandbox.
use layerfs_bridge::control::SdkValue;
use layerfs_control::calls::table::Kind::{Bool, Int, Name};
use layerfs_control::calls::table::{a, call, opt, Call};
use layerfs_control::calls::Context;
use layerfs_control::common::outcome::Outcome;

/// Its table entry.
pub(crate) const CALL: Call = call(
    "sandbox",
    "close",
    &[a("name", Name)],
    &[
        opt("force", Bool),
        opt("generation", Int),
        opt("dry_run", Bool),
    ],
    "stop a Sandbox",
    run,
);

/// Runs `sandbox.close`.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    layerfs_control::sandbox::family::sandbox(context, "close", args)
}
