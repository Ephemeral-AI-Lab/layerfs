//! `machine.stats`: what a machine has and uses now.
use crate::calls::table::Kind::Name;
use crate::calls::table::{a, call, Call};
use crate::calls::Context;
use layerfs_bridge::control::SdkValue;
use layerfs_home::names::MachineName;
use layerfs_home::outcome::Outcome;
#[cfg(unix)]
use layerfs_sandbox::inspect::machine_stats;

/// Its table entry.
pub(crate) const CALL: Call = call(
    "machine",
    "stats",
    &[a("machine", Name)],
    &[],
    "a machine's observed processors, memory and data disk, its Stores' sizes and live Sandboxes",
    run,
)
.quiet();

/// Runs `machine.stats`: `{ machine, platform, observed_at, worker,
/// resources, data_disk, stores, sandboxes, complete, reason? }`, read in
/// one bounded request where the machine's data is. It takes no lock or
/// lease, starts nothing and writes nothing, the call log included
/// (layerfs-project#164).
#[cfg(unix)]
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let machine = MachineName::parse(args.get_text("machine").unwrap_or(""))?;
    machine_stats(&context.home, &machine)
}

// Windows: the machine stats read waits for the monitoring port.
#[cfg(windows)]
fn run(_context: &mut Context<'_>, _args: &SdkValue) -> Outcome {
    Err(layerfs_home::outcome::refused("Unsupported", "machine.stats reads on the unix face today (#178)"))
}
