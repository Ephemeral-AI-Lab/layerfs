//! `docker` power: start and stop the worker container.
use super::cli::docker;
use super::network::published_port;
use super::unix::{guest, inspect, run};
use crate::error::{Code, MachineError};
use crate::setup::{tool, worker};
use crate::spec::{Bundle, Running, Spec, DATA, STATE, WORKER_PORT};
use std::path::Path;
use std::time::Duration;

/// How long loading the bundle's Sandbox image into the Engine may take.
const LOAD: Duration = Duration::from_secs(300);

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
    let secret_file = format!("LAYERFS_WORKER_SECRET={STATE}/secret");
    let tmpfs = format!("{STATE}:mode=0700");
    // On a Linux Engine the worker shares the host's network: it listens on
    // a host loopback port and dials its Sandboxes' loopback-published
    // daemon ports as published (`LAYERFS_WORKER_HOST_NETWORK`; the
    // bridge gateway `host.docker.internal` names does not reach a port
    // published on 127.0.0.1, terminal pass lane K3). Docker Desktop's
    // worker publishes its port and dials `host.docker.internal`.
    let host_network = cfg!(target_os = "linux");
    let port = if host_network { tool::free_port()? } else { 0 };
    let listen = match host_network {
        true => format!("LAYERFS_WORKER_LISTEN=127.0.0.1:{port}"),
        false => format!("LAYERFS_WORKER_LISTEN=0.0.0.0:{WORKER_PORT}"),
    };
    let publish = format!("127.0.0.1::{WORKER_PORT}");
    #[rustfmt::skip]
    let mut args = vec![
        "run", "-d", "--pull", "never", "--name", &spec.vm_name, "--init",
        "--label", ROLE, "--label", &bundle_label,
    ];
    if host_network {
        // a Linux Engine has no `host.docker.internal` of its own: the name
        // `reach` gives a loopback PostgreSQL server (#535 §7.4) resolves to
        // the bridge's gateway, the host, for the worker's Store calls and
        // its Sandboxes' reach check
        #[rustfmt::skip]
        args.extend([
            "--network", "host", "--add-host", "host.docker.internal:host-gateway",
            "-e", "LAYERFS_WORKER_HOST_NETWORK=1",
        ]);
    } else {
        args.extend(["-p", &publish]);
    }
    #[rustfmt::skip]
    args.extend([
        "-v", &socket, "-v", &volume, "-v", &bundle_mount, "--tmpfs", &tmpfs,
        "-e", &listen, "-e", &secret_file, "--entrypoint", "sh", image, "-c", ENTRY,
    ]);
    run(spec, &args, "starting the worker container")?.answer("starting the worker container")?;
    let port = if host_network {
        port
    } else {
        let ports = run(
            spec,
            &["port", &spec.vm_name, &format!("{WORKER_PORT}/tcp")],
            "docker port",
        )?
        .answer("docker port")?;
        published_port(&ports).ok_or_else(|| {
            MachineError::new(
                Code::Failed,
                format!("the worker {} publishes no port ({ports})", spec.vm_name),
            )
        })?
    };
    let guest = guest(spec)?;
    let secret = tool::secret()?;
    guest.write_file(&spec.tools, &format!("{STATE}/secret"), &secret)?;
    let address = worker::loopback(port);
    worker::ready(spec, &guest, address)?;
    Ok(Running {
        endpoint: crate::spec::Endpoint {
            address,
            secret,
            forward_pid: None,
        },
        lasting: spec.state,
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

/// Loads the docker-archive `archive` (the bundle's Sandbox image) into the
/// Engine once and answers the image id Docker reports, `sha256:<64 hex>`:
/// the image of the worker container and of Sandboxes when the docker
/// machine names none (#535 P5). One attempt; the caller records the id.
pub(crate) fn load_image(spec: &Spec, archive: &Path) -> Result<String, MachineError> {
    let file = archive.to_str().ok_or_else(|| {
        MachineError::new(
            Code::InvalidInput,
            format!("the image archive {} is not UTF-8", archive.display()),
        )
    })?;
    let what = "loading the bundle's Sandbox image";
    let said = tool::run(docker(spec, &["load", "-i", file])?, None, LOAD, what)?.answer(what)?;
    loaded_id(&said).ok_or_else(|| {
        MachineError::new(
            Code::Failed,
            format!("docker load named no image id: {said}"),
        )
    })
}

/// The image id in `docker load`'s `Loaded image ID: sha256:…` (or
/// `Loaded image: sha256:…`) line.
fn loaded_id(said: &str) -> Option<String> {
    said.lines()
        .filter(|line| line.starts_with("Loaded image"))
        .filter_map(|line| line.split_once("sha256:").map(|(_, hex)| hex.trim()))
        .find(|hex| hex.len() == 64 && hex.bytes().all(|b| b.is_ascii_hexdigit()))
        .map(|hex| format!("sha256:{}", hex.to_ascii_lowercase()))
}
