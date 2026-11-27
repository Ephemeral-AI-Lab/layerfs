//! `machine.get`: a machine's record and state; with `--with live`, one
//! probe of its provider and one hello to its worker.
use crate::dispatch::table::Kind::Name;
use crate::dispatch::table::{a, call, Call};
#[cfg(feature = "machine")]
use crate::dispatch::Context;
use crate::dispatch::{Form, Section};
#[cfg(feature = "machine")]
use layerfs_home::names::MachineName;
#[cfg(feature = "machine")]
use layerfs_home::reply::outcome::Outcome;
#[cfg(feature = "machine")]
use layerfs_protocol::sdk::SdkValue;

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
.parts(&super::list::ON)
.short_when(&[crate::dispatch::answer::form::OBSERVED]);

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
    &["disk_free_bytes", "tool_version", "machine_state", "worker"],
)]);

/// Runs `machine.get`: the probe runs only with `--with live`.
#[cfg(feature = "machine")]
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let machine = MachineName::parse(args.get_text("machine").unwrap_or(""))?;
    layerfs_machine::with_users::get(
        &context.home,
        &machine,
        context.with("live"),
        &layerfs_sandbox::users::MACHINE,
    )
}

// without the `machine` feature (the guest worker's build) it is
// refused before any effect
crate::dispatch::guard::host_only::host_only!("get");
