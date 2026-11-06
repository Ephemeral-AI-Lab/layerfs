//! `workspace.mount`: mount a Branch in a Sandbox.
use crate::calls::table::Kind::Name;
use crate::calls::table::{a, call, opt, Call};
use crate::calls::Context;
use layerfs_bridge::control::SdkValue;
use layerfs_home::outcome::Outcome;

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
    let (home, events) = context.parts();
    layerfs_sandbox::workspace::calls::mount(home, events, args)
}
