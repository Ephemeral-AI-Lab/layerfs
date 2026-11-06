//! `docker` on macOS and Linux: one worker container, `<vm_name>`, started
//! detached and privileged (the port of `docker/macos.js`; D-8). It mounts the
//! Store volume `<vm_name>-data` at `/layerfs/data` and the bundle read-only;
//! never the host home or Docker's socket. Its worker listens behind a secret on a port published to
//! the host's loopback only. The secret is written into a private tmpfs after
//! the container starts, so it is in no command line and no container config.
use super::cli::{docker, engine_version};
use crate::error::{Code, MachineError};
use crate::setup::guest::Guest;
use crate::setup::{tool, worker};
use crate::setup::{with_worker, State, Status};
use crate::spec::Spec;
use std::time::Duration;

const CALL: Duration = Duration::from_secs(30);
pub(super) fn run(spec: &Spec, args: &[&str], what: &str) -> Result<tool::Output, MachineError> {
    tool::run(docker(spec, args)?, None, CALL, what)
}

/// The container's `{{.State.Status}}` and role label, or `None` when it does not exist.
pub(super) fn inspect(spec: &Spec) -> Result<Option<(String, String)>, MachineError> {
    let format = "{{.State.Status}}\t{{index .Config.Labels \"layerfs.role\"}}";
    let out = run(
        spec,
        &["inspect", "-f", format, &spec.vm_name],
        "docker inspect",
    )?;
    if !out.ok() {
        // Docker 29 says `error: no such object`; older Engines capitalise it
        let said = out.stderr.to_ascii_lowercase();
        if said.contains("no such object") || said.contains("no such container") {
            return Ok(None);
        }
        return Err(MachineError::new(
            Code::Failed,
            format!("docker inspect failed: {}", out.said()),
        ));
    }
    let line = out.stdout.trim();
    let (state, role) = line.split_once('\t').unwrap_or((line, ""));
    Ok(Some((state.to_owned(), role.to_owned())))
}

pub(crate) fn guest(spec: &Spec) -> Result<Guest, MachineError> {
    Ok(Guest::Docker {
        socket: spec.tools.docker_socket.clone(),
        container: spec.vm_name.clone(),
    })
}

pub(crate) fn status(spec: &Spec) -> Result<Status, MachineError> {
    let version = engine_version(spec);
    if version.is_none() {
        return Ok(Status {
            state: State::Stopped,
            free_disk: None,
            tool_version: None,
            down_reason: Some(format!(
                "the Docker Engine at {} does not answer",
                spec.tools.docker_socket.display()
            )),
        });
    }
    let kernel = match inspect(spec)? {
        None => State::Stopped,
        Some((state, _)) if state == "running" => State::Running,
        Some((state, _)) if state == "dead" => State::Failed,
        Some(_) => State::Stopped,
    };
    let free = match (kernel, guest(spec)) {
        (State::Running, Ok(guest)) => worker::free_disk(spec, &guest),
        _ => None,
    };
    Ok(with_worker(spec, kernel, free, version))
}
