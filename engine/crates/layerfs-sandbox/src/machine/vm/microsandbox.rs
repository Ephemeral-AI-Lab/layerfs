//! `microsandbox` (`msb`, libkrun microVMs): the VM is one sandbox record,
//! its data disk a named `disk` volume `<vm>-data` that survives stop and
//! start, mounted at `/layerfs/data`, and its worker port published as
//! `127.0.0.1:<port>:7070`. No host folder is mounted. The name is the only
//! identity: every read goes through `msb inspect` by name. A failed create
//! is reported as it is; nothing is replayed or replaced on a guess.
use super::shared::{
    call, checked, data_volume, exists, mib, new_port, not_found, recorded_port, CALL, CREATE,
};
use crate::machine::error::{kept, Code, MachineError};
use crate::machine::guest::Guest;
use crate::machine::probe::first_line;
use crate::machine::setup::{with_worker, State, Status};
use crate::machine::spec::{Bundle, Lasting, Running, Spec, DATA, WORKER_PORT};
use crate::machine::{facts, tool, worker};

const CLI: &str = "msb";
const DEFAULT_IMAGE: &str = "debian:trixie-slim";

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

pub(crate) fn create(spec: &Spec, bundle: &Bundle) -> Result<Running, MachineError> {
    tool::find(&spec.tools, CLI)?;
    if observe(spec)? != State::Missing {
        return Err(exists(spec, "the microsandbox"));
    }
    let port = new_port(spec)?;
    let volume = data_volume(spec);
    let cpus = spec.cpus.to_string();
    let memory = mib(spec.memory);
    let mount = format!("{volume}:{DATA}:kind=disk,size={}", mib(spec.disk));
    let publish = format!("127.0.0.1:{port}:{WORKER_PORT}");
    let image = spec.os_image.as_deref().unwrap_or(DEFAULT_IMAGE);
    #[rustfmt::skip]
    let args = [
        "create", "--name", &spec.vm_name, "--cpus", &cpus, "--memory", &memory,
        "--mount-named", &mount, "-p", &publish, "--label", "layerfs.role=machine", image,
    ];
    let made = call(spec, CLI, &args, CREATE)?;
    if !made.ok() {
        if observe(spec)? != State::Missing {
            return Err(MachineError::new(
                Code::Failed,
                format!("msb create failed: {}", made.said()),
            )
            .kept());
        }
        // nothing was made under the name: a volume it may have made goes (one cleanup)
        let _ = call(spec, CLI, &["volume", "remove", &volume], CALL);
        return Err(MachineError::new(
            Code::Failed,
            format!("msb create failed: {}", made.said()),
        ));
    }
    kept(boot_and_bring_up(spec, bundle, port))
}

fn boot_and_bring_up(spec: &Spec, bundle: &Bundle, port: u16) -> Result<Running, MachineError> {
    if observe(spec)? != State::Running {
        checked(spec, CLI, &["start", &spec.vm_name], CREATE)?;
    }
    let endpoint = worker::bring_up(spec, &guest(spec), bundle, port, None)?;
    Ok(Running {
        endpoint,
        lasting: Lasting {
            port: Some(port),
            ..spec.state
        },
    })
}

pub(crate) fn start(spec: &Spec, bundle: &Bundle) -> Result<Running, MachineError> {
    let port = recorded_port(spec)?;
    if observe(spec)? == State::Missing {
        return Err(not_found(spec));
    }
    boot_and_bring_up(spec, bundle, port)
}

pub(crate) fn stop(spec: &Spec) -> Result<(), MachineError> {
    if observe(spec)? == State::Running {
        checked(spec, CLI, &["stop", &spec.vm_name], CREATE)?;
    }
    Ok(())
}

/// The sandbox record, then its data volume.
pub(crate) fn remove(spec: &Spec) -> Result<(), MachineError> {
    if observe(spec)? != State::Missing {
        checked(spec, CLI, &["remove", "--force", &spec.vm_name], CREATE)?;
    }
    let out = call(spec, CLI, &["volume", "remove", &data_volume(spec)], CALL)?;
    if out.ok() || out.said().to_lowercase().contains("not found") {
        return Ok(());
    }
    Err(MachineError::new(
        Code::Failed,
        format!("msb volume remove failed: {}", out.said()),
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
