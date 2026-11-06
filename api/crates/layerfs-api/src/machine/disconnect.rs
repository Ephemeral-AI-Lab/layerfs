//! `machine.disconnect`: forget an ssh machine; its data root stays.
use crate::calls::table::Kind::Name;
use crate::calls::table::{a, call, Call};
use crate::calls::Context;
use layerfs_bridge::control::SdkValue;
use layerfs_home::outcome::Outcome;

/// Its table entry.
pub(crate) const CALL: Call = call(
    "machine",
    "disconnect",
    &[a("name", Name)],
    &[],
    "forget an ssh machine; its data root stays",
    run,
);

/// Runs `machine.disconnect`.
#[cfg(feature = "machine")]
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    layerfs_control::machine::disconnect(&context.home, args)
}

/// Without the `machine` feature (the guest worker's build) it is
/// refused before any effect.
#[cfg(not(feature = "machine"))]
fn run(_context: &mut Context<'_>, _args: &SdkValue) -> Outcome {
    Err(layerfs_home::outcome::refused(
        "Unsupported",
        "machine.disconnect is not built yet (#533 step 6)",
    ))
}
