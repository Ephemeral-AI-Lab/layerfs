//! `machine.start`: start a stopped machine and its worker; a running one answers at once.
use crate::calls::table::Kind::Name;
use crate::calls::table::{a, call, Call, PROGRESS};
use crate::calls::Context;
use layerfs_bridge::control::SdkValue;
#[cfg(feature = "machine")]
use layerfs_home::names::MachineName;
use layerfs_home::outcome::Outcome;

/// Its table entry.
pub(crate) const CALL: Call = call(
    "machine",
    "start",
    &[a("name", Name)],
    &[PROGRESS],
    "start a stopped machine and its worker; a running one answers at once",
    run,
);

/// Runs `machine.start`.
#[cfg(feature = "machine")]
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let name = MachineName::parse(args.get_text("name").unwrap_or(""))?;
    let (home, events) = context.parts();
    layerfs_control::machine::start_with_progress(home, events, &name)
}

/// Without the `machine` feature (the guest worker's build) it is
/// refused before any effect.
#[cfg(not(feature = "machine"))]
fn run(_context: &mut Context<'_>, _args: &SdkValue) -> Outcome {
    Err(layerfs_home::outcome::refused(
        "Unsupported",
        "machine.start is not built yet (#533 step 6)",
    ))
}
