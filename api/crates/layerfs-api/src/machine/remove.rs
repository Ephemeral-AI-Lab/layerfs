//! `machine.remove`: delete a VM machine and its disk; a machine of a
//! platform removed in 2.0 is only forgotten.
use crate::calls::table::Kind::Name;
use crate::calls::table::{a, call, Call};
use crate::calls::Context;
use layerfs_bridge::control::SdkValue;
#[cfg(feature = "machine")]
use layerfs_home::names::MachineName;
use layerfs_home::outcome::Outcome;

/// Its table entry.
pub(crate) const CALL: Call = call(
    "machine",
    "remove",
    &[a("name", Name)],
    &[],
    "delete a VM machine and its disk; one of a removed platform is only forgotten",
    run,
);

/// Runs `machine.remove`.
#[cfg(feature = "machine")]
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let name = MachineName::parse(args.get_text("name").unwrap_or(""))?;
    layerfs_control::machine::remove(&context.home, &name)
}

/// Without the `machine` feature (the guest worker's build) it is
/// refused before any effect.
#[cfg(not(feature = "machine"))]
fn run(_context: &mut Context<'_>, _args: &SdkValue) -> Outcome {
    Err(layerfs_home::outcome::refused(
        "Unsupported",
        "machine.remove is not built yet (#533 step 6)",
    ))
}
