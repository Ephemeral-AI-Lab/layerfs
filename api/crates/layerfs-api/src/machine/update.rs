//! `machine.update`: resize a VM, or set a built-in machine's image, Docker socket and daemon.
use crate::calls::table::Kind::{Int, Name, Size, Text};
use crate::calls::table::{a, call, opt, Call};
use crate::calls::Context;
use layerfs_bridge::control::SdkValue;
use layerfs_home::outcome::Outcome;

/// Its table entry.
pub(crate) const CALL: Call = call(
    "machine",
    "update",
    &[a("name", Name)],
    &[
        opt("cpus", Int),
        opt("memory", Size),
        opt("image", Text),
        opt("docker", Text),
        opt("wsl", Text),
        opt("daemon", Text),
    ],
    "resize a VM, or set a built-in machine's image, Docker socket and daemon",
    run,
);

/// Runs `machine.update`.
#[cfg(feature = "machine")]
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let text = |name: &str| args.get_text(name).map(str::to_owned);
    let request = layerfs_machine::MachineUpdate {
        name: text("name").unwrap_or_default(),
        cpus: args.get_int("cpus"),
        memory: args.get_int("memory"),
        image: text("image"),
        docker: text("docker"),
        wsl: text("wsl"),
        daemon: text("daemon"),
    };
    layerfs_control::machine::update(&context.home, &request)
}

/// Without the `machine` feature (the guest worker's build) it is
/// refused before any effect.
#[cfg(not(feature = "machine"))]
fn run(_context: &mut Context<'_>, _args: &SdkValue) -> Outcome {
    Err(layerfs_home::outcome::refused(
        "Unsupported",
        "machine.update is not built yet (#533 step 6)",
    ))
}
