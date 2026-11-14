//! `machine.load`: copy an image from the host's Docker to a machine without creating a Sandbox.
use crate::calls::table::Kind::{Image, Name};
use crate::calls::table::{a, call, Call, PROGRESS};
use crate::calls::Context;
use layerfs_bridge::control::SdkValue;
#[cfg(feature = "machine")]
use layerfs_home::names::MachineName;
use layerfs_home::outcome::Outcome;

/// Its table entry.
pub(crate) const CALL: Call = call(
    "machine",
    "load",
    &[a("name", Name), a("image", Image)],
    &[PROGRESS],
    "copy an image from the host's Docker to a machine without creating a Sandbox",
    run,
);

/// Runs `machine.load`.
#[cfg(feature = "machine")]
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let name = MachineName::parse(args.get_text("name").unwrap_or(""))?;
    let image = args.get_text("image").unwrap_or("").to_owned();
    let (home, events) = context.parts();
    layerfs_control::machine::load(home, events, &name, &image)
}

/// Without the `machine` feature (the guest worker's build) it is
/// refused before any effect.
#[cfg(not(feature = "machine"))]
fn run(_context: &mut Context<'_>, _args: &SdkValue) -> Outcome {
    Err(layerfs_home::outcome::refused(
        "Unsupported",
        "machine.load runs on the host",
    ))
}
