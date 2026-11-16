//! `machine.space`: where a machine's data disk goes.
use crate::calls::table::Kind::Name;
use crate::calls::table::{a, call, Call};
use crate::calls::Context;
use layerfs_bridge::control::SdkValue;
use layerfs_home::names::MachineName;
use layerfs_home::outcome::Outcome;
use layerfs_sandbox::inspect::machine_space;

/// Its table entry.
pub(crate) const CALL: Call = call(
    "machine",
    "space",
    &[a("machine", Name)],
    &[],
    "where a machine's data disk goes: its Stores, Sandboxes, images and logs, and what is not LayerFS's",
    run,
)
.quiet();

/// Runs `machine.space`: `{ machine, platform, observed_at, disk, layerfs,
/// outside, parts, stores, sandboxes, unreadable, complete, reason? }`,
/// one bounded walk of LayerFS's own folders where the machine's data is,
/// each size the blocks a part takes. `outside` is what the data disk uses
/// that is not LayerFS's. It takes no lock or lease, starts nothing, dials
/// no daemon, opens no file and writes nothing, the call log included.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let machine = MachineName::parse(args.get_text("machine").unwrap_or(""))?;
    machine_space(&context.home, &machine)
}
