//! `sandbox.close`: stop a Sandbox.
use crate::calls::table::Kind::{Bool, Name};
use crate::calls::table::{a, call, opt, Call};
use crate::calls::Context;
use layerfs_bridge::control::SdkValue;
use layerfs_home::outcome::Outcome;

/// Its table entry.
pub(crate) const CALL: Call = call(
    "sandbox",
    "close",
    &[a("name", Name)],
    &[opt("force", Bool)],
    "stop a Sandbox",
    run,
);

/// Runs `sandbox.close`.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let (home, events) = context.parts();
    layerfs_sandbox::family::close(home, events, args)
}
