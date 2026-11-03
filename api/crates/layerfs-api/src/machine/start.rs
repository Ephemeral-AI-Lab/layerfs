//! `machine.start`: start a stopped machine and its worker; a running one answers at once.
use layerfs_bridge::control::SdkValue;
use layerfs_control::calls::table::Kind::Name;
use layerfs_control::calls::table::{a, call, Call};
use layerfs_control::calls::Context;
use layerfs_control::common::outcome::Outcome;

/// Its table entry.
pub(crate) const CALL: Call = call(
    "machine",
    "start",
    &[a("name", Name)],
    &[],
    "start a stopped machine and its worker; a running one answers at once",
    run,
);

/// Runs `machine.start`.
#[cfg(feature = "machine")]
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    layerfs_control::machine::start(context, args)
}

/// Without the `machine` feature (the guest worker's build) it is
/// refused before any effect.
#[cfg(not(feature = "machine"))]
fn run(_context: &mut Context<'_>, _args: &SdkValue) -> Outcome {
    Err(layerfs_control::common::outcome::refused(
        "Unsupported",
        "machine.start is not built yet (#533 step 6)",
    ))
}
