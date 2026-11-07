//! `machine.connect`: connect an ssh host as a machine.
use crate::calls::table::Kind::{Int, Name, Text};
use crate::calls::table::{a, call, opt, Call};
use crate::calls::Context;
use layerfs_bridge::control::SdkValue;
use layerfs_home::outcome::Outcome;

/// Its table entry.
pub(crate) const CALL: Call = call(
    "machine",
    "connect",
    &[a("name", Name), a("host", Text)],
    &[
        opt("user", Text),
        opt("port", Int),
        opt("identity", Text),
        opt("known_hosts", Text),
        opt("root", Text),
    ],
    "SSH connections are permanently disabled; existing machines can be inspected or disconnected",
    run,
);

/// Runs `machine.connect`.
#[cfg(feature = "machine")]
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let text = |name: &str| args.get_text(name).map(str::to_owned);
    let request = layerfs_machine::MachineConnect {
        name: text("name").unwrap_or_default(),
        host: text("host").unwrap_or_default(),
        user: text("user"),
        port: args.get_int("port"),
        identity: text("identity"),
        known_hosts: args.get_text("known_hosts").map(std::path::PathBuf::from),
        root: text("root"),
    };
    layerfs_control::machine::connect(&context.home, &request)
}

/// Without the `machine` feature (the guest worker's build) it is
/// refused before any effect.
#[cfg(not(feature = "machine"))]
fn run(_context: &mut Context<'_>, _args: &SdkValue) -> Outcome {
    Err(layerfs_home::outcome::refused(
        "Unsupported",
        "machine.connect is not built yet (#533 step 6)",
    ))
}
