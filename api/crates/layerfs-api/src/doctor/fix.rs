//! `doctor.fix`: remove one leftover `layerfs doctor` listed, under its guard.
use crate::calls::table::Kind::Text;
use crate::calls::table::{a, call, Call};
use crate::calls::Context;
use layerfs_bridge::control::SdkValue;
use layerfs_home::outcome::Outcome;

/// Its table entry.
pub(crate) const CALL: Call = call(
    "doctor",
    "fix",
    &[a("id", Text)],
    &[],
    "remove one leftover `layerfs doctor` listed, under its guard",
    run,
);

/// Runs `doctor.fix`.
#[cfg(unix)]
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let id = args.get_text("id").unwrap_or("");
    layerfs_control::doctor::leftover_fix::fix(&context.home, id)
}

// Windows: the leftover fix waits for the monitoring port (#180).
#[cfg(windows)]
fn run(_context: &mut Context<'_>, _args: &SdkValue) -> Outcome {
    Err(layerfs_home::outcome::refused("Unsupported", "doctor.fix walks leftovers on the unix face today (#180)"))
}
