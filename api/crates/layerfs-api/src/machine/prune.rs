//! `machine.prune`: remove a Sandbox image from a machine, or every unused one.
use crate::calls::table::Kind::{Bool, Image, Name};
use crate::calls::table::{a, call, opt, Call};
use crate::calls::Context;
use layerfs_bridge::control::SdkValue;
#[cfg(feature = "machine")]
use layerfs_home::names::MachineName;
use layerfs_home::outcome::Outcome;

/// Its table entry.
pub(crate) const CALL: Call = call(
    "machine",
    "prune",
    &[a("name", Name), opt("image", Image)],
    &[opt("unused", Bool)],
    "remove a Sandbox image from a machine, or every unused one",
    run,
);

/// Runs `machine.prune`.
#[cfg(feature = "machine")]
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let name = MachineName::parse(args.get_text("name").unwrap_or(""))?;
    let unused = args.get_bool("unused");
    layerfs_control::machine::prune(&context.home, &name, args.get_text("image"), unused)
}

/// Without the `machine` feature (the guest worker's build) it is
/// refused before any effect.
#[cfg(not(feature = "machine"))]
fn run(_context: &mut Context<'_>, _args: &SdkValue) -> Outcome {
    Err(layerfs_home::outcome::refused(
        "Unsupported",
        "machine.prune runs on the host",
    ))
}
