//! `docker` power: start and stop the worker container.
use super::network::published_port;
use super::unix::{guest, inspect, run};
use crate::error::{Code, MachineError};
use crate::setup::{tool, worker};
use crate::spec::{Bundle, Running, Spec, DATA, STATE, WORKER_PORT};

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
