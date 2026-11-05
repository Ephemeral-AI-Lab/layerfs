//! `sandbox.create`: start a named Sandbox of a project on a machine
//! (default: its sqlite Store's, or docker with a PostgreSQL Store).
use layerfs_bridge::control::SdkValue;
use layerfs_control::calls::table::Kind::{Name, Project, Text};
use layerfs_control::calls::table::{a, call, opt, Call};
use layerfs_control::calls::Context;
use layerfs_control::common::outcome::Outcome;

/// Its table entry.
pub(crate) const CALL: Call = call(
    "sandbox",
    "create",
    &[a("project", Project), a("name", Name), opt("machine", Name)],
    &[opt("image", Text), opt("isolation", Text)],
    "start a named Sandbox of a project on a machine (default: its sqlite Store's, or docker)",
    run,
);

/// Runs `sandbox.create`.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    layerfs_control::sandbox::family::sandbox(context, "create", args)
}
