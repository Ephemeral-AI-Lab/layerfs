//! `logs.machine`: a machine's worker log, one window per call.
use crate::calls::table::Kind::Name;
use crate::calls::table::{a, call, Call, WINDOW};
use crate::calls::Context;
use layerfs_bridge::control::SdkValue;
use layerfs_home::names::MachineName;
use layerfs_home::outcome::Outcome;
use layerfs_sandbox::inspect::{machine_log, Window};

/// Its table entry.
pub(crate) const CALL: Call = call(
    "logs",
    "machine",
    &[a("machine", Name)],
    &WINDOW,
    "a machine's worker log: its last lines, or the lines after or before a cursor",
    run,
);

/// Runs `logs.machine`: `{ lines: [{ text, continued? }], cursor, before?,
/// has_more }`, read in the machine's worker. It takes no lock or lease,
/// starts nothing and writes nothing (layerfs-project#178).
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let machine = MachineName::parse(args.get_text("machine").unwrap_or(""))?;
    machine_log(&context.home, &machine, &Window::of(args))
}
