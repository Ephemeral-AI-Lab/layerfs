//! `logs.sandbox`: a Sandbox's daemon log, one window per call.
use crate::calls::table::Kind::Name;
use crate::calls::table::{a, call, Call, WINDOW};
use crate::calls::Context;
use layerfs_bridge::control::SdkValue;
use layerfs_home::outcome::Outcome;
use layerfs_sandbox::inspect::{sandbox_log, Window};

/// Its table entry.
pub(crate) const CALL: Call = call(
    "logs",
    "sandbox",
    &[a("sandbox", Name)],
    &WINDOW,
    "a Sandbox's daemon log: its last lines, or the lines after or before a cursor",
    run,
);

/// Runs `logs.sandbox`: `{ lines: [{ text, continued? }], cursor, before?,
/// has_more }`, read where the Sandbox's run folder is. It takes no lock
/// or lease, asks no daemon and writes nothing (layerfs-project#178).
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let sandbox = args.get_text("sandbox").unwrap_or("");
    sandbox_log(&context.home, sandbox, &Window::of(args))
}
