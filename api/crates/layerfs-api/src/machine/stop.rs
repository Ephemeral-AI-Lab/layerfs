//! `machine.stop`: stop a machine's worker and VM; its disks stay.
use crate::calls::table::Kind::{Bool, Name};
use crate::calls::table::{a, call, opt, Call};
use crate::calls::Context;
use layerfs_bridge::control::SdkValue;
#[cfg(feature = "machine")]
use layerfs_home::names::MachineName;
use layerfs_home::outcome::Outcome;

/// Its table entry.
pub(crate) const CALL: Call = call(
    "machine",
    "stop",
    &[a("name", Name)],
    &[opt("force", Bool)],
    "stop a machine's worker and VM; its disks stay",
    run,
);

/// Runs `machine.stop`.
#[cfg(feature = "machine")]
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let name = MachineName::parse(args.get_text("name").unwrap_or(""))?;
    if args.get_bool("force") {
        layerfs_control::machine::stop_force(&context.home, &name)
    } else {
        layerfs_control::machine::stop(&context.home, &name)
    }
}

/// Without the `machine` feature (the guest worker's build) it is
/// refused before any effect.
#[cfg(not(feature = "machine"))]
fn run(_context: &mut Context<'_>, _args: &SdkValue) -> Outcome {
    Err(layerfs_home::outcome::refused(
        "Unsupported",
        "machine.stop is not built yet (#533 step 6)",
    ))
}
