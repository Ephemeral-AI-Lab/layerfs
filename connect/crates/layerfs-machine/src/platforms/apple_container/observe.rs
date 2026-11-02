//! `apple-container` (Apple's `container` CLI; Apple silicon, macOS only):
//! the VM is one container whose main process is a shell that sleeps and
//! ends on SIGTERM (the idle worker's stop; [`MAIN`]), its data disk a named
//! volume `<vm>-data` sized at create and mounted at `/layerfs/data`, and its
//! worker port published as `127.0.0.1:<port>:7070`. No host folder is
//! mounted. Facts verified against `container` 1.6.1 (#527): `inspect`
//! prints a JSON array whose `status.state` is the state; the guest's
//! `/proc/sys` is read-only until remounted (the shared preparation does);
//! `delete --force` ends a running container; `volume delete` removes the disk.
use crate::error::{Code, MachineError};
use crate::facts;
use crate::kind::HostOs;
use crate::probe::first_line;
use crate::setup::guest::Guest;
use crate::setup::vm::{call, CALL};
use crate::setup::{tool, worker};
use crate::setup::{with_worker, State, Status};
use crate::spec::Spec;

pub(super) const CLI: &str = "container";
pub(super) const DEFAULT_IMAGE: &str = "debian:trixie-slim";
/// The container's main process. As PID 1 it ignores signals it has no
/// handler for, and the container has no `CAP_SYS_BOOT`, so the trap is how
/// an idle worker ends the container (`kill -TERM 1`).
pub(super) const MAIN: &str = "trap 'exit 0' TERM; sleep infinity & wait";
/// FUSE, cgroups and loopback in the guest need these (#512).
pub(super) const CAPABILITIES: [&str; 6] = [
    "--cap-add",
    "SYS_ADMIN",
    "--cap-add",
    "SYS_RESOURCE",
    "--cap-add",
    "NET_ADMIN",
];

/// The VM's state from `container inspect`, read strictly.
pub(crate) fn observe(spec: &Spec) -> Result<State, MachineError> {
    let out = call(spec, CLI, &["inspect", &spec.vm_name], CALL)?;
    if !out.ok() {
        let said = out.said().to_lowercase();
        if said.contains("not found") || said.contains("no such") {
            return Ok(State::Missing);
        }
        return Err(MachineError::new(
            Code::Failed,
            format!("container inspect failed: {}", out.said()),
        ));
    }
    if out.stdout.trim() == "[]" {
        return Ok(State::Missing);
    }
    let state = facts::json_string(&out.stdout, &["status", "state"])
        .ok()
        .flatten()
        .or_else(|| facts::json_string(&out.stdout, &["status"]).ok().flatten())
        .ok_or_else(|| {
            MachineError::new(
                Code::Failed,
                "the container CLI's inspect answer is not the shape this adapter knows",
            )
        })?;
    Ok(match state.to_lowercase().as_str() {
        "running" => State::Running,
        "stopped" => State::Stopped,
        _ => State::Failed,
    })
}

pub(crate) fn guest(spec: &Spec) -> Guest {
    Guest::AppleContainer {
        vm: spec.vm_name.clone(),
    }
}

pub(super) fn host_check(spec: &Spec) -> Result<(), MachineError> {
    let mut missing = Vec::new();
    if HostOs::current() != HostOs::Macos {
        missing.push("host:macos".to_owned());
    } else if std::env::consts::ARCH != "aarch64" {
        missing.push("host:apple-silicon".to_owned());
    }
    if tool::lookup(&spec.tools, CLI).is_none() {
        missing.push(format!("tool:{CLI}"));
    }
    if missing.is_empty() {
        return Ok(());
    }
    Err(MachineError::missing(
        "an apple-container machine cannot run on this host",
        missing,
    ))
}

pub(crate) fn status(spec: &Spec) -> Result<Status, MachineError> {
    let version = tool::lookup(&spec.tools, CLI)
        .and_then(|p| first_line(tool::command(&p, ["--version"]), false));
    let kernel = observe(spec)?;
    let free = (kernel == State::Running)
        .then(|| worker::free_disk(spec, &guest(spec)))
        .flatten();
    Ok(with_worker(spec, kernel, free, version))
}
