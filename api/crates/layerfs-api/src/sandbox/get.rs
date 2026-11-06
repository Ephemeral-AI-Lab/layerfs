//! `sandbox.get`: a Sandbox and its Workspaces.
use layerfs_bridge::control::SdkValue;
use layerfs_control::calls::table::Kind::Name;
use layerfs_control::calls::table::{a, call, Call, PAGE};
use layerfs_control::calls::Context;
use layerfs_home::outcome::Outcome;

/// Its table entry.
pub(crate) const CALL: Call = call(
    "sandbox",
    "get",
    &[a("name", Name)],
    &PAGE,
    "a Sandbox and its Workspaces",
    run,
);

/// Runs `sandbox.get`.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    layerfs_sandbox::family::sandbox(context, "get", args)
}
