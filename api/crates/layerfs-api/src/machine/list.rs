//! `machine.list`: every machine, from the home's records; with `--with
//! live`, each probed once.
use crate::calls::table::{call, Call};
#[cfg(feature = "machine")]
use crate::calls::Context;
use crate::calls::{Form, Part, Section};
#[cfg(feature = "machine")]
use layerfs_protocol::provision::install::SdkValue;
#[cfg(feature = "machine")]
use layerfs_home::outcome::Outcome;

/// What is on a machine, by name: `machine.get` and each item here.
pub(super) const ON: [(&str, Part); 3] = [
    ("supports", Part::Scalars("support")),
    ("stores", Part::Scalars("store")),
    ("sandboxes", Part::Scalars("sandbox")),
];

/// One machine: its name and platform, its state when not ready.
const ITEM: Form = Form::of(
    &["machine", "platform"],
    &["state", "op", "reason", "fix", "observed_at"],
    &[
        "machine",
        "platform",
        "state",
        "op",
        "reason",
        "fix",
        "observed_at",
        "backend",
        "supports",
        "loopback",
        "cpus",
        "memory_bytes",
        "disk_bytes",
        "image",
        "os_image",
        "docker",
        "wsl",
        "daemon",
        "message",
        "stores",
        "stores_count",
        "sandboxes",
        "sandboxes_count",
        "created_at",
    ],
)
.parts(&ON)
.short_when(&[crate::calls::form::OBSERVED]);
const FORM: Form = Form::of(&["items"], &[], &["items"]).parts(&[("items", Part::Rows(&ITEM))]);

/// Its table entry.
pub(crate) const CALL: Call = call(
    "machine",
    "list",
    &[],
    &[],
    "every machine, from the home's records",
    run,
)
.form(&FORM)
.with(&[Section::item("live", &["observed_at"])]);

/// Runs `machine.list`: the probes run only with `--with live`.
#[cfg(feature = "machine")]
fn run(context: &mut Context<'_>, _args: &SdkValue) -> Outcome {
    layerfs_machine::with_users::list(&context.home, context.with("live"))
}

// without the `machine` feature (the guest worker's build) it is
// refused before any effect
crate::calls::host_only::host_only!("list");
