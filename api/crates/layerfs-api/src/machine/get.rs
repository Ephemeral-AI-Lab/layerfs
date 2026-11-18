//! `machine.get`: a machine's record and state; with `--with live`, one
//! probe of its provider and one hello to its worker.
use crate::calls::table::Kind::Name;
use crate::calls::table::{a, call, Call};
use crate::calls::{Context, Form, Section};
use layerfs_bridge::control::SdkValue;
#[cfg(feature = "machine")]
use layerfs_home::names::MachineName;
use layerfs_home::outcome::Outcome;

/// Its answer: the platform and the state with its companion, then what
/// the record holds and what is on the machine.
const FORM: Form = Form::of(
    &["platform", "state"],
    &["op", "reason", "fix", "observed_at"],
    &[
        "platform",
        "state",
        "op",
        "reason",
        "fix",
        "observed_at",
        "machine",
        "backend",
        "supports",
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
.parts(&super::list::ON)
.short_when(&[crate::calls::form::OBSERVED]);

/// Its table entry.
pub(crate) const CALL: Call = call(
    "machine",
    "get",
    &[a("machine", Name)],
    &[],
    "a machine's record and state",
    run,
)
.form(&FORM)
.with(&[Section::own(
    "live",
    &["disk_free_bytes", "tool_version", "worker"],
)]);

/// Runs `machine.get`: the probe runs only with `--with live`.
#[cfg(feature = "machine")]
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let machine = MachineName::parse(args.get_text("machine").unwrap_or(""))?;
    layerfs_control::machine::get(&context.home, &machine, context.with("live"))
}

/// Without the `machine` feature (the guest worker's build) it is
/// refused before any effect.
#[cfg(not(feature = "machine"))]
fn run(_context: &mut Context<'_>, _args: &SdkValue) -> Outcome {
    Err(layerfs_home::outcome::refused(
        "Unsupported",
        "machine.get runs on the host",
    ))
}
