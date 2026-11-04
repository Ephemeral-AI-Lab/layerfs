//! `machine.update`: resize a VM, or set a built-in machine's image, Docker socket and daemon.
use layerfs_bridge::control::SdkValue;
use layerfs_control::calls::table::Kind::{Int, Name, Size, Text};
use layerfs_control::calls::table::{a, call, opt, Call};
use layerfs_control::calls::Context;
use layerfs_control::common::outcome::Outcome;

/// Its table entry.
pub(crate) const CALL: Call = call(
    "machine",
    "update",
    &[a("name", Name)],
    &[
        opt("cpus", Int),
        opt("memory", Size),
        opt("image", Text),
        opt("docker", Text),
        opt("wsl", Text),
        opt("daemon", Text),
    ],
    "resize a VM, or set a built-in machine's image, Docker socket and daemon",
    run,
);

/// Runs `machine.update`.
#[cfg(feature = "machine")]
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    layerfs_control::machine::update(context, args)
}

/// Without the `machine` feature (the guest worker's build) it is
/// refused before any effect.
#[cfg(not(feature = "machine"))]
fn run(_context: &mut Context<'_>, _args: &SdkValue) -> Outcome {
    Err(layerfs_control::common::outcome::refused(
        "Unsupported",
        "machine.update is not built yet (#533 step 6)",
    ))
}
