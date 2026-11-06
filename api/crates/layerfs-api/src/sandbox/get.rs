//! `sandbox.get`: a Sandbox and its Workspaces.
use crate::calls::table::Kind::Name;
use crate::calls::table::{a, call, Call, PAGE};
use crate::calls::Context;
use layerfs_bridge::control::SdkValue;
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
    let (home, events) = context.parts();
    layerfs_sandbox::family::get(home, events, args)
}
