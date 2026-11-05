//! `sandbox.scratch`: one bounded pass over one machine's settled Sandbox
//! scratch. `--dry-run` lists exactly the roots a real pass deletes; both
//! come from the same predicate (#549).
use layerfs_bridge::control::SdkValue;
use layerfs_control::calls::table::Kind::{Bool, Name};
use layerfs_control::calls::table::{a, call, opt, Call};
use layerfs_control::calls::Context;
use layerfs_control::common::outcome::Outcome;

/// Its table entry.
pub(crate) const CALL: Call = call(
    "sandbox",
    "scratch",
    &[a("machine", Name)],
    &[opt("dry_run", Bool)],
    "list or remove a machine's settled Sandbox scratch",
    run,
);

/// Runs `sandbox.scratch`.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    layerfs_control::sandbox::family::sandbox(context, "scratch", args)
}
