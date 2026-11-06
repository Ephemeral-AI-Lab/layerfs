//! `microsandbox` (`msb`, libkrun microVMs): the VM is one sandbox record,
//! its data disk a named `disk` volume `<vm>-data` that survives stop and
//! start, mounted at `/layerfs/data`, and its worker port published as
//! `127.0.0.1:<port>:7070`. No host folder is mounted. The name is the only
//! identity: every read goes through `msb inspect` by name. A failed create
//! is reported as it is; nothing is replayed or replaced on a guess.
use crate::error::{Code, MachineError};
use crate::facts;
use crate::probe::first_line;
use crate::setup::guest::Guest;
use crate::setup::vm::{call, CALL};
use crate::setup::{tool, worker};
use crate::setup::{with_worker, State, Status};
use crate::spec::Spec;

pub(super) const CLI: &str = "msb";
pub(super) const DEFAULT_IMAGE: &str = "debian:trixie-slim";

/// The VM's state from `msb inspect --format json`.
pub(crate) fn observe(spec: &Spec) -> Result<State, MachineError> {
    let out = call(
        spec,
        CLI,
        &["inspect", "--format", "json", &spec.vm_name],
        CALL,
    )?;
    if !out.ok() {
        let said = out.said().to_lowercase();
        if said.contains("not found") || said.contains("no such sandbox") {
            return Ok(State::Missing);
        }
        return Err(MachineError::new(
            Code::Failed,
            format!("msb inspect failed: {}", out.said()),
        ));
    }
    let status = facts::json_string(&out.stdout, &["status"])
        .ok()
        .flatten()
        .ok_or_else(|| MachineError::new(Code::Failed, "msb inspect's answer names no status"))?;
    Ok(match status.to_lowercase().as_str() {
        "running" => State::Running,
        "stopped" | "created" => State::Stopped,
        _ => State::Failed,
    })
}

pub(crate) fn guest(spec: &Spec) -> Guest {
    Guest::Microsandbox {
        vm: spec.vm_name.clone(),
    }
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
