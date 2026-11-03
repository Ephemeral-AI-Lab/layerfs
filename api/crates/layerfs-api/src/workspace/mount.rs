//! `workspace.mount`: mount a Branch in a Sandbox.
use layerfs_bridge::control::SdkValue;
use layerfs_control::calls::table::Kind::Name;
use layerfs_control::calls::table::{a, call, opt, Call};
use layerfs_control::calls::Context;
use layerfs_control::common::outcome::Outcome;

/// Its table entry.
pub(crate) const CALL: Call = call(
    "workspace",
    "mount",
    &[a("sandbox", Name), a("branch", Name)],
    &[opt("name", Name)],
    "mount a Branch in a Sandbox",
    run,
);

/// Runs `workspace.mount`.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    layerfs_control::workspace::calls::workspace(context, "mount", args)
}
