//! `machine.update`: set a machine's image, or a built-in machine's Docker
//! socket, WSL distribution and daemon.
use crate::calls::table::Kind::{Int, Name, Size, Text};
use crate::calls::table::{a, call, opt, Call};
#[cfg(feature = "machine")]
use crate::calls::Context;
use crate::calls::{Form, Policy};
#[cfg(feature = "machine")]
use layerfs_bridge::control::SdkValue;
#[cfg(feature = "machine")]
use layerfs_home::outcome::Outcome;

/// Its answer: `ok`; the long form adds the settings as recorded.
const FORM: Form = Form::of(
    &["ok"],
    &[],
    &[
        "ok",
        "outcome",
        "machine",
        "platform",
        "cpus",
        "memory_bytes",
        "disk_bytes",
        "image",
        "docker",
        "wsl",
        "daemon",
    ],
);

/// Its table entry.
pub(crate) const CALL: Call = call(
    "machine",
    "update",
    &[a("machine", Name)],
    &[
        opt("cpus", Int),
        opt("memory", Size),
        opt("image", Text),
        opt("docker", Text),
        opt("wsl", Text),
        opt("daemon", Text),
    ],
    "set a machine's image, or a built-in machine's Docker socket, WSL distribution and daemon",
    run,
)
.policy(Policy::Machine("machine"))
.form(&FORM);

/// Runs `machine.update`.
#[cfg(feature = "machine")]
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let text = |name: &str| args.get_text(name).map(str::to_owned);
    let request = layerfs_machine::MachineUpdate {
        name: text("machine").unwrap_or_default(),
        cpus: args.get_int("cpus"),
        memory: args.get_int("memory"),
        image: text("image"),
        docker: text("docker"),
        wsl: text("wsl"),
        daemon: text("daemon"),
    };
    layerfs_control::machine::update(&context.home, &request)
}

// without the `machine` feature (the guest worker's build) it is
// refused before any effect
crate::calls::host_only::host_only!("update");
