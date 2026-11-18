//! `machine.stop`: stop a machine's worker and VM; its disks stay.
use crate::calls::table::Kind::{Bool, Name};
use crate::calls::table::{a, call, opt, Call, PROGRESS};
use crate::calls::{Context, Form};
use layerfs_bridge::control::SdkValue;
#[cfg(feature = "machine")]
use layerfs_home::names::MachineName;
use layerfs_home::outcome::Outcome;

/// Its answer: `ok`, `unchanged` when it was stopped already, and what a
/// forced stop stopped beside it.
const FORM: Form = Form::of(
    &[],
    &[
        "ok",
        "unchanged",
        "stopped_sandboxes",
        "stopped_sandboxes_count",
        "unknown_workspaces",
        "unknown_workspaces_count",
    ],
    &[
        "ok",
        "unchanged",
        "stopped_sandboxes",
        "stopped_sandboxes_count",
        "unknown_workspaces",
        "unknown_workspaces_count",
        "outcome",
        "machine",
        "forced",
    ],
);

/// Its table entry.
pub(crate) const CALL: Call = call(
    "machine",
    "stop",
    &[a("machine", Name)],
    &[opt("force", Bool), PROGRESS],
    "stop a machine's worker and VM; its disks stay",
    run,
)
.form(&FORM);

/// Runs `machine.stop`: `{ phase, done, total }` events, phases `check` and
/// `stop`.
#[cfg(feature = "machine")]
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let machine = MachineName::parse(args.get_text("machine").unwrap_or(""))?;
    let force = args.get_bool("force");
    let (home, events) = context.parts();
    layerfs_control::machine::stop(home, events, &machine, force)
}

/// Without the `machine` feature (the guest worker's build) it is
/// refused before any effect.
#[cfg(not(feature = "machine"))]
fn run(_context: &mut Context<'_>, _args: &SdkValue) -> Outcome {
    Err(layerfs_home::outcome::refused(
        "Unsupported",
        "machine.stop runs on the host",
    ))
}
