//! `machine.create`: create a VM machine.
use crate::calls::table::Kind::{Bool, Int, Name, Size, Text};
use crate::calls::table::{a, call, opt, Call};
use crate::calls::Context;
use layerfs_bridge::control::SdkValue;
use layerfs_home::outcome::Outcome;

/// Its table entry.
pub(crate) const CALL: Call = call(
    "machine",
    "create",
    &[a("name", Name), a("platform", Name)],
    &[
        opt("cpus", Int),
        opt("memory", Size),
        opt("disk", Size),
        opt("image", Text),
        opt("nested", Bool),
        opt("host", Name),
    ],
    "create an enabled VM machine (apple-container, microsandbox, lima)",
    run,
);

/// Runs `machine.create`.
#[cfg(feature = "machine")]
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let (home, events) = context.parts();
    layerfs_control::machine::create(home, events, args)
}

/// Without the `machine` feature (the guest worker's build) it is
/// refused before any effect.
#[cfg(not(feature = "machine"))]
fn run(_context: &mut Context<'_>, _args: &SdkValue) -> Outcome {
    Err(layerfs_home::outcome::refused(
        "Unsupported",
        "machine.create is not built yet (#533 step 6)",
    ))
}
