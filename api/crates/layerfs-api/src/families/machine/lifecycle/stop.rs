//! `machine.stop`: stop a machine's worker and VM; its disks stay.
use crate::dispatch::table::Kind::{Bool, Name};
use crate::dispatch::table::{a, call, opt, Call, PROGRESS};
#[cfg(feature = "machine")]
use crate::dispatch::Context;
use crate::dispatch::Form;
#[cfg(feature = "machine")]
use layerfs_home::names::MachineName;
#[cfg(feature = "machine")]
use layerfs_home::outcome::Outcome;
#[cfg(feature = "machine")]
use layerfs_protocol::sdk::SdkValue;

/// Its answer: `ok`, `unchanged` when it was stopped already, what a
/// forced stop stopped beside it, and from the host worker's drain: the
/// uncertain write outcomes it could not record up to then (§8.5) and the
/// admitted writes still running, when there are any.
const FORM: Form = Form::of(
    &[],
    &[
        "ok",
        "unchanged",
        "stopped_sandboxes",
        "stopped_sandboxes_count",
        "unknown_workspaces",
        "unknown_workspaces_count",
        "unrecorded_writes",
        "unrecorded_writes_count",
        "running_writes",
        "running_writes_note",
    ],
    &[
        "ok",
        "unchanged",
        "stopped_sandboxes",
        "stopped_sandboxes_count",
        "unknown_workspaces",
        "unknown_workspaces_count",
        "unrecorded_writes",
        "unrecorded_writes_count",
        "running_writes",
        "running_writes_note",
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
    layerfs_machine::with_users::stop(
        home,
        events,
        &machine,
        force,
        &layerfs_sandbox::users::MACHINE,
    )
}

// without the `machine` feature (the guest worker's build) it is
// refused before any effect
crate::dispatch::guard::host_only::host_only!("stop");
