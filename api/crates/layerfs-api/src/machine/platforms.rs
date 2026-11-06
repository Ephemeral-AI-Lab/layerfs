//! `machine.platforms`: the platforms this host and build support, and what each is missing.
use crate::calls::table::{call, Call};
use crate::calls::Context;
use layerfs_bridge::control::SdkValue;
use layerfs_home::outcome::Outcome;

/// Its table entry.
pub(crate) const CALL: Call = call(
    "machine",
    "platforms",
    &[],
    &[],
    "the platforms this host and build support, and what each is missing",
    run,
);

/// Runs `machine.platforms`.
#[cfg(feature = "machine")]
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    layerfs_machine::platforms(&context.home, args)
}

/// Without the `machine` feature (the guest worker's build) it is
/// refused before any effect.
#[cfg(not(feature = "machine"))]
fn run(_context: &mut Context<'_>, _args: &SdkValue) -> Outcome {
    Err(layerfs_home::outcome::refused(
        "Unsupported",
        "machine.platforms is not built yet (#533 step 6)",
    ))
}
