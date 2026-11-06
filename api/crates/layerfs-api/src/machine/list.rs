//! `machine.list`: every machine, with what it supports.
use layerfs_bridge::control::SdkValue;
use layerfs_control::calls::table::{call, Call};
use layerfs_control::calls::Context;
use layerfs_home::outcome::Outcome;

/// Its table entry.
pub(crate) const CALL: Call = call(
    "machine",
    "list",
    &[],
    &[],
    "every machine, with what it supports",
    run,
);

/// Runs `machine.list`.
#[cfg(feature = "machine")]
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    layerfs_control::machine::list(context, args)
}

/// Without the `machine` feature (the guest worker's build) it is
/// refused before any effect.
#[cfg(not(feature = "machine"))]
fn run(_context: &mut Context<'_>, _args: &SdkValue) -> Outcome {
    Err(layerfs_home::outcome::refused(
        "Unsupported",
        "machine.list is not built yet (#533 step 6)",
    ))
}
