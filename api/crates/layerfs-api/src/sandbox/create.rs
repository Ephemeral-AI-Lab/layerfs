//! `sandbox.create`: start a named Sandbox of a project on a machine (default: its sqlite Store's, or native).
use crate::calls::table::Kind::{Name, Project, Text};
use crate::calls::table::{a, call, opt, Call};
use crate::calls::Context;
use layerfs_bridge::control::SdkValue;
use layerfs_home::outcome::Outcome;

/// Its table entry.
pub(crate) const CALL: Call = call(
    "sandbox",
    "create",
    &[a("project", Project), a("name", Name), opt("machine", Name)],
    &[opt("image", Text)],
    "start a named Sandbox of a project on a machine (default: its sqlite Store's, or native)",
    run,
);

/// Runs `sandbox.create`.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let (home, events) = context.parts();
    layerfs_sandbox::family::create(home, events, args)
}
