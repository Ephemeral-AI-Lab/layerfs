//! `docker` on macOS and Linux: one worker container, `<vm_name>`, started
//! detached (the port of `docker/macos.js`). It mounts Docker's own socket, the Store
//! volume `<vm_name>-data` at `/layerfs/data` and the bundle read-only; never
//! the host home. Its worker listens behind a secret on a port published to
//! the host's loopback only. The secret is written into a private tmpfs after
//! the container starts, so it is in no command line and no container config.
use super::cli::{docker, engine_version};
use crate::machine::error::{Code, MachineError};
use crate::machine::guest::Guest;
use crate::machine::setup::{with_worker, State, Status};
use crate::machine::spec::{Bundle, Running, Spec, DATA, STATE, WORKER_PORT};
use crate::machine::{tool, worker};
use std::time::Duration;

const CALL: Duration = Duration::from_secs(30);
/// The label that marks a container as a machine's worker.
const ROLE: &str = "layerfs.role=machine-worker";
/// The Engine's socket as the Engine's kernel sees it (inside Docker
/// Desktop's VM, or the Linux host), which the worker's Sandbox backend
/// uses; a Mac's own socket path means nothing there.
const VM_DOCKER: &str = "/var/run/docker.sock";
/// The container's main process waits (bounded) for its secret, then becomes the worker.
const ENTRY: &str = r#"i=0
while [ ! -s /layerfs/state/secret ]; do i=$((i+1)); [ $i -le 600 ] || exit 1; sleep 0.1; done
exec /layerfs/bundle/layerfs worker"#;

fn run(spec: &Spec, args: &[&str], what: &str) -> Result<tool::Output, MachineError> {
    tool::run(docker(spec, args)?, None, CALL, what)
}

/// The container's `{{.State.Status}}` and role label, or `None` when it does not exist.
fn inspect(spec: &Spec) -> Result<Option<(String, String)>, MachineError> {
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

/// A fresh worker container: an earlier one of ours is replaced (its Store
/// volume stays); a container of that name that is not a worker is refused.
pub(crate) fn start(spec: &Spec, bundle: &Bundle) -> Result<Running, MachineError> {
    let image = spec.os_image.as_deref().ok_or_else(|| {
        MachineError::new(
            Code::InvalidInput,
            "the docker machine's worker container needs an image",
        )
    })?;
    if let Some((_, role)) = inspect(spec)? {
        if format!("layerfs.role={role}") != ROLE {
            return Err(MachineError::new(
                Code::Exists,
                format!(
                    "the container {} is not a LayerFS worker; it is left alone",
                    spec.vm_name
                ),
            ));
        }
        run(
            spec,
            &["rm", "-f", &spec.vm_name],
            "removing the old worker container",
        )?
        .answer("removing the old worker container")?;
    }
    let bundle_mount = format!("{}:/layerfs/bundle:ro", bundle.dir.display());
    let volume = format!("{}-data:{DATA}", spec.vm_name);
    let socket = format!("{VM_DOCKER}:{VM_DOCKER}");
    let bundle_label = format!("layerfs.bundle={}", bundle.hash);
    let publish = format!("127.0.0.1::{WORKER_PORT}");
    let listen = format!("LAYERFS_WORKER_LISTEN=0.0.0.0:{WORKER_PORT}");
    let secret_file = format!("LAYERFS_WORKER_SECRET={STATE}/secret");
    let tmpfs = format!("{STATE}:mode=0700");
    #[rustfmt::skip]
    let args = [
        "run", "-d", "--pull", "never", "--name", &spec.vm_name, "--init",
        "--label", ROLE, "--label", &bundle_label, "-p", &publish,
        "-v", &socket, "-v", &volume, "-v", &bundle_mount, "--tmpfs", &tmpfs,
        "-e", &listen, "-e", &secret_file, "--entrypoint", "sh", image, "-c", ENTRY,
    ];
    run(spec, &args, "starting the worker container")?.answer("starting the worker container")?;
    let ports = run(
        spec,
        &["port", &spec.vm_name, &format!("{WORKER_PORT}/tcp")],
        "docker port",
    )?
    .answer("docker port")?;
    let port = published_port(&ports).ok_or_else(|| {
        MachineError::new(
            Code::Failed,
            format!("the worker {} publishes no port ({ports})", spec.vm_name),
        )
    })?;
    let guest = guest(spec)?;
    let secret = tool::secret()?;
    guest.write_file(&spec.tools, &format!("{STATE}/secret"), &secret)?;
    let address = worker::loopback(port);
    worker::ready(spec, &guest, address)?;
    Ok(Running {
        endpoint: crate::machine::spec::Endpoint {
            address,
            secret,
            forward_pid: None,
        },
        lasting: spec.state,
    })
}

/// The host port of `docker port`'s answer (`127.0.0.1:55012`, maybe a second IPv6 line).
pub(crate) fn published_port(answer: &str) -> Option<u16> {
    answer
        .lines()
        .find_map(|line| line.rsplit_once(':')?.1.trim().parse().ok())
}

pub(crate) fn guest(spec: &Spec) -> Result<Guest, MachineError> {
    Ok(Guest::Docker {
        socket: spec.tools.docker_socket.clone(),
        container: spec.vm_name.clone(),
    })
}

/// Stops the worker container; Docker itself and the Store volume stay.
pub(crate) fn stop(spec: &Spec) -> Result<(), MachineError> {
    if inspect(spec)?.is_none() {
        return Ok(());
    }
    run(
        spec,
        &["stop", "-t", "5", &spec.vm_name],
        "stopping the worker container",
    )?
    .answer("stopping the worker container")?;
    Ok(())
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
