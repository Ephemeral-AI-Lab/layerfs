//! `lima` (`limactl`, `vmType: vz`, macOS only): one Lima instance per
//! machine, made from a template written to `limactl create`'s stdin.
//!
//! - The data disk is a raw Lima disk that Lima formats ext4 on first attach
//!   and mounts at `/mnt/lima-<disk>`; it is bind-mounted at `/layerfs/data`
//!   after every boot. Lima finds a formatted disk by its ext4 label
//!   `lima-<disk>`, which holds 16 bytes, so the disk is named `lfs` and 8 hex
//!   digits of the VM name (a longer label is cut, never found, and formatted
//!   again at every boot; found on Lima 2.2.1, #527).
//! - The template mounts nothing (`mounts: []`): the guest never sees the
//!   host home. Its one port forward is guest `127.0.0.1:7070` to host
//!   `127.0.0.1:<port>`; every other guest port is ignored.
//! - `nested` adds `nestedVirtualization` (macOS 15+, M3 or newer): such a
//!   machine hosts Firecracker machines, and the bundle's `firecracker/`
//!   folder (binary, kernel, root filesystem, guest key), when it has one, is
//!   copied from the host straight to `/layerfs/data/firecracker` before the
//!   worker starts. No guest's bundle copy carries that folder.
//! - Commands go through ssh with Lima's own `ssh.config`, then `sudo -n`, so
//!   the quoting is this crate's, not the CLI's.
use crate::error::{Code, MachineError};
use crate::facts;
use crate::platform::HostOs;
use crate::probe::{first_line, nested_missing};
use crate::setup::guest::Guest;
use crate::setup::vm::{call, checked, CALL};
use crate::setup::{tool, worker};
use crate::setup::{with_worker, State, Status};
use crate::spec::Spec;
use std::path::{Path, PathBuf};

pub(super) const CLI: &str = "limactl";

/// The Lima disk of a machine: a name whose `lima-` label fits 16 bytes.
pub(crate) fn disk_name(vm_name: &str) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in vm_name.bytes() {
        hash = (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3);
    }
    format!("lfs{:08x}", hash >> 32)
}

/// The instance's state and folder, from `limactl list`.
pub(crate) fn observe(spec: &Spec) -> Result<(State, Option<PathBuf>), MachineError> {
    let out = call(
        spec,
        CLI,
        &["list", "--format", "{{.Status}}\t{{.Dir}}", &spec.vm_name],
        CALL,
    )?;
    let line = out.stdout.trim();
    if line.is_empty() || out.stderr.contains("No instance matching") {
        if out.ok() || out.stderr.contains("No instance matching") {
            return Ok((State::Missing, None));
        }
        return Err(MachineError::new(
            Code::Failed,
            format!("limactl list failed: {}", out.said()),
        ));
    }
    let (status, dir) = line.split_once('\t').unwrap_or((line, ""));
    let state = match status {
        "Running" => State::Running,
        "Broken" => State::Failed,
        _ => State::Stopped,
    };
    Ok((state, (!dir.is_empty()).then(|| PathBuf::from(dir))))
}

/// The guest of a running instance.
pub(crate) fn guest(spec: &Spec) -> Result<Guest, MachineError> {
    match observe(spec)? {
        (State::Running, Some(dir)) => Ok(Guest::Lima {
            dir,
            vm: spec.vm_name.clone(),
        }),
        (state, _) => Err(MachineError::new(
            Code::Unavailable,
            format!("the lima machine {} is {}", spec.name, state.as_str()),
        )),
    }
}

/// The ssh options and destination of a detached forward through a running
/// Lima instance (`setup/forward.rs`): Lima's own `ssh.config`, without its
/// shared connection, so the forward is its own ssh process.
pub(crate) fn tunnel(dir: &Path, vm: &str) -> (Vec<String>, String) {
    #[rustfmt::skip]
    let options: Vec<String> = [
        "-F", &dir.join("ssh.config").display().to_string(),
        "-o", "ControlMaster=no", "-o", "ControlPath=none",
    ].map(String::from).to_vec();
    (options, format!("lima-{vm}"))
}

/// What a lima machine needs on this host before anything is made.
pub(super) fn host_check(spec: &Spec) -> Result<(), MachineError> {
    let mut missing = Vec::new();
    if HostOs::current() != HostOs::Macos {
        missing.push("host:macos".to_owned());
    }
    if spec.nested {
        missing.extend(nested_missing(HostOs::current()));
    }
    if tool::lookup(&spec.tools, CLI).is_none() {
        missing.push(format!("tool:{CLI}"));
    }
    if missing.is_empty() {
        return Ok(());
    }
    Err(MachineError::missing(
        format!(
            "a lima machine{} cannot run on this host",
            if spec.nested {
                " with nested virtualization"
            } else {
                ""
            }
        ),
        missing,
    ))
}

pub(super) fn disk_exists(spec: &Spec, disk: &str) -> Result<bool, MachineError> {
    let text = checked(spec, CLI, &["disk", "list", "--json"], CALL)?;
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let name = facts::json_string(line, &["name"])
            .map_err(|e| MachineError::new(Code::Failed, format!("limactl disk list: {e}")))?;
        if name.as_deref() == Some(disk) {
            return Ok(true);
        }
    }
    Ok(false)
}

pub(crate) fn status(spec: &Spec) -> Result<Status, MachineError> {
    let version = tool::lookup(&spec.tools, CLI)
        .and_then(|p| first_line(tool::command(&p, ["--version"]), false));
    let (kernel, dir) = observe(spec)?;
    let free = match (kernel, dir) {
        (State::Running, Some(dir)) => worker::free_disk(
            spec,
            &Guest::Lima {
                dir,
                vm: spec.vm_name.clone(),
            },
        ),
        _ => None,
    };
    Ok(with_worker(spec, kernel, free, version))
}
