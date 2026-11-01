//! `apple-container` (Apple's `container` CLI; Apple silicon, macOS only):
//! the VM is one container whose main process sleeps, its data disk a named
//! volume `<vm>-data` sized at create and mounted at `/layerfs/data`, and its
//! worker port published as `127.0.0.1:<port>:7070`. No host folder is
//! mounted. Facts verified against `container` 1.6.1 (#527): `inspect`
//! prints a JSON array whose `status.state` is the state; the guest's
//! `/proc/sys` is read-only until remounted (the shared preparation does);
//! `delete --force` ends a running container; `volume delete` removes the disk.
use super::shared::{
    call, checked, data_volume, exists, mib, new_port, not_found, recorded_port, CALL, CREATE,
};
use crate::machine::error::{kept, Code, MachineError};
use crate::machine::guest::Guest;
use crate::machine::kind::HostOs;
use crate::machine::probe::first_line;
use crate::machine::setup::{with_worker, State, Status};
use crate::machine::spec::{Bundle, Lasting, Running, Spec, DATA, WORKER_PORT};
use crate::machine::{facts, tool, worker};

const CLI: &str = "container";
const DEFAULT_IMAGE: &str = "debian:trixie-slim";
/// FUSE, cgroups and loopback in the guest need these (#512).
const CAPABILITIES: [&str; 6] = [
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

fn host_check(spec: &Spec) -> Result<(), MachineError> {
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

pub(crate) fn create(spec: &Spec, bundle: &Bundle) -> Result<Running, MachineError> {
    host_check(spec)?;
    if observe(spec)? != State::Missing {
        return Err(exists(spec, "the container"));
    }
    let volume = data_volume(spec);
    if call(spec, CLI, &["volume", "inspect", &volume], CALL)?.ok() {
        return Err(MachineError::new(
            Code::Exists,
            format!("the volume {volume} already exists; it is left alone"),
        ));
    }
    let port = new_port(spec)?;
    checked(
        spec,
        CLI,
        &["volume", "create", "-s", &mib(spec.disk), &volume],
        CREATE,
    )?;
    let cpus = spec.cpus.to_string();
    let memory = mib(spec.memory);
    let mount = format!("type=volume,source={volume},target={DATA}");
    let publish = format!("127.0.0.1:{port}:{WORKER_PORT}");
    let image = spec.os_image.as_deref().unwrap_or(DEFAULT_IMAGE);
    let mut args = vec![
        "run",
        "--detach",
        "--name",
        &spec.vm_name,
        "--cpus",
        &cpus,
        "--memory",
        &memory,
    ];
    args.extend(CAPABILITIES);
    args.extend([
        "--mount",
        &mount,
        "--publish",
        &publish,
        "--label",
        "layerfs.role=machine",
    ]);
    args.extend([image, "sleep", "infinity"]);
    let made = call(spec, CLI, &args, CREATE)?;
    if !made.ok() {
        if observe(spec)? != State::Missing {
            return Err(MachineError::new(
                Code::Failed,
                format!("container run failed: {}", made.said()),
            )
            .kept());
        }
        // nothing was made under the name: the volume this create made goes (one cleanup)
        let _ = call(spec, CLI, &["volume", "delete", &volume], CALL);
        return Err(MachineError::new(
            Code::Failed,
            format!("container run failed: {}", made.said()),
        ));
    }
    kept(bring_up(spec, bundle, port))
}

fn bring_up(spec: &Spec, bundle: &Bundle, port: u16) -> Result<Running, MachineError> {
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
    match observe(spec)? {
        State::Missing => return Err(not_found(spec)),
        State::Running => {}
        State::Stopped | State::Failed => {
            checked(spec, CLI, &["start", &spec.vm_name], CREATE)?;
        }
    }
    bring_up(spec, bundle, port)
}

pub(crate) fn stop(spec: &Spec) -> Result<(), MachineError> {
    if observe(spec)? == State::Running {
        checked(spec, CLI, &["stop", &spec.vm_name], CREATE)?;
    }
    Ok(())
}

/// The container, then its data volume.
pub(crate) fn remove(spec: &Spec) -> Result<(), MachineError> {
    if observe(spec)? != State::Missing {
        checked(spec, CLI, &["delete", "--force", &spec.vm_name], CREATE)?;
    }
    let volume = data_volume(spec);
    if call(spec, CLI, &["volume", "inspect", &volume], CALL)?.ok() {
        checked(spec, CLI, &["volume", "delete", &volume], CALL)?;
    }
    Ok(())
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
