//! `machine.remove`: delete a VM machine and its disk; of a machine of a
//! platform removed in 2.0 only the record goes.
use crate::calls::table::Kind::Name;
use crate::calls::table::{a, call, Call, PROGRESS};
use crate::calls::{Context, Form};
use layerfs_bridge::control::SdkValue;
#[cfg(feature = "machine")]
use layerfs_home::names::MachineName;
use layerfs_home::outcome::Outcome;

/// Its answer: `ok`; `message` says when only the record went.
const FORM: Form = Form::of(&[], &["ok"], &["ok", "outcome", "machine", "message"]);

/// Its table entry.
pub(crate) const CALL: Call = call(
    "machine",
    "remove",
    &[a("machine", Name)],
    &[PROGRESS],
    "delete a VM machine and its disk",
    run,
)
.form(&FORM);

/// Runs `machine.remove`: `{ phase, done, total }` events, phases `check`
/// and `delete`. The machine's own PostgreSQL addresses go with it.
#[cfg(feature = "machine")]
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let machine = MachineName::parse(args.get_text("machine").unwrap_or(""))?;
    let (home, events) = context.parts();
    layerfs_control::machine::remove(home, events, &machine)
}

/// Without the `machine` feature (the guest worker's build) it is
/// refused before any effect.
#[cfg(not(feature = "machine"))]
fn run(_context: &mut Context<'_>, _args: &SdkValue) -> Outcome {
    Err(layerfs_home::outcome::refused(
        "Unsupported",
        "machine.remove runs on the host",
    ))
}
