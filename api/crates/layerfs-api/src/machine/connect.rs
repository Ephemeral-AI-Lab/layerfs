//! `machine.connect`: connect an ssh host as a machine.
use layerfs_bridge::control::SdkValue;
use layerfs_control::calls::table::Kind::{Int, Name, Text};
use layerfs_control::calls::table::{a, call, opt, Call};
use layerfs_control::calls::Context;
use layerfs_control::common::outcome::Outcome;

/// Its table entry.
pub(crate) const CALL: Call = call(
    "machine",
    "connect",
    &[a("name", Name), a("host", Text)],
    &[
        opt("user", Text),
        opt("port", Int),
        opt("identity", Text),
        opt("known_hosts", Text),
        opt("root", Text),
    ],
    "SSH connections are permanently disabled; existing machines can be inspected or disconnected",
    run,
);

/// Runs `machine.connect`.
#[cfg(feature = "machine")]
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    layerfs_control::machine::connect(context, args)
}

/// Without the `machine` feature (the guest worker's build) it is
/// refused before any effect.
#[cfg(not(feature = "machine"))]
fn run(_context: &mut Context<'_>, _args: &SdkValue) -> Outcome {
    Err(layerfs_control::common::outcome::refused(
        "Unsupported",
        "machine.connect is not built yet (#533 step 6)",
    ))
}
