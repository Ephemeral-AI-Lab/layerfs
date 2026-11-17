//! `docker` power: start and stop the worker container. It is privileged,
//! in a cgroup namespace of its own, so its worker runs `namespaces`
//! Sandboxes inside it (D-8): root, `/dev/fuse`, a writable cgroup v2 tree
//! with `cgroup.kill`, and its runtime root on the data volume, which can
//! hold an overlay's upper layer. It never sets `user.max_user_namespaces`,
//! which would change the whole Docker host: every Sandbox command runs
//! under the filter that refuses a user namespace instead.
use super::cli::docker;
use super::network::published_port;
use super::unix::{guest, inspect, run};
use crate::error::{Code, MachineError};
use crate::setup::{tool, worker};
use crate::spec::{Bundle, Running, Spec, DATA, STATE, WORKER_LOG, WORKER_PORT};
use std::path::Path;
use std::time::Duration;

/// How long loading the bundle's Sandbox image into the Engine may take.
const LOAD: Duration = Duration::from_secs(300);

/// The label that marks a container as a machine's worker.
const ROLE: &str = "layerfs.role=machine-worker";
/// The container's main process makes the data volume's folders as a VM
/// guest's preparation does (`setup::worker`, without its user-namespace
/// limit) and the worker's log as every launcher does
/// ([`worker::log_start`]), writes its pid (the worker's, once it becomes
/// it) where the readiness look reads it, waits (bounded) for its secret,
/// then becomes the worker, whose output is appended to that log: on the
/// data volume, it stays when the next start replaces the container.
/// Before anything else is exec'd into the container's cgroup namespace,
/// its processes move to a leaf and its root gives Sandboxes the memory and
/// pids controllers ([`worker::CGROUP_LEAF`], layerfs-project#182 L4); each
/// start makes a new container, so each start does it.
fn entry() -> String {
    format!(
        r#"set -e
umask 022
mkdir -p /layerfs/data/store /layerfs/data/sandbox
chmod 755 /layerfs/data /layerfs/data/store /layerfs/data/sandbox
{}
{}
echo $$ > /layerfs/state/worker.pid
i=0
while [ ! -s /layerfs/state/secret ]; do i=$((i+1)); [ $i -le 600 ] || exit 1; sleep 0.1; done
exec /layerfs/bundle/layerfs worker >> "$LAYERFS_WORKER_LOG" 2>&1"#,
        worker::log_start(),
        worker::CGROUP_LEAF
    )
}

/// The end of the worker's log when its container no longer runs: a worker
/// that ends stops its container, which `docker exec` cannot enter, so the
/// log is copied out of the stopped container (`docker cp` reads its data
/// volume) into the machine's folder, read and removed. `None` while the
/// container runs.
fn ended(spec: &Spec) -> Option<String> {
    let (state, _) = inspect(spec).ok()??;
    if !matches!(state.as_str(), "exited" | "dead") {
        return None;
    }
    let copy = spec.work_dir.join("worker.log.ended");
    let _ = std::fs::create_dir_all(&spec.work_dir);
    let from = format!("{}:{WORKER_LOG}", spec.vm_name);
    let what = "copying the ended worker's log";
    let copied = copy
        .to_str()
        .is_some_and(|to| run(spec, &["cp", &from, to], what).is_ok_and(|out| out.ok()));
    let said = match copied {
        true => tool::tail(&copy, 8),
        false => format!("its container is {state} and its log could not be copied out"),
    };
    let _ = std::fs::remove_file(&copy);
    Some(said)
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
    let bundle_label = format!("layerfs.bundle={}", bundle.hash);
    let secret_file = format!("LAYERFS_WORKER_SECRET={STATE}/secret");
    let log = format!("LAYERFS_WORKER_LOG={WORKER_LOG}");
    let entry = entry();
    let tmpfs = format!("{STATE}:mode=0700");
    // On a Linux Engine the worker shares the host's network and listens on
    // a host loopback port. Docker Desktop's worker publishes its port.
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
        "--privileged", "--cgroupns", "private",
        "--label", ROLE, "--label", &bundle_label,
        // what the entry says before it becomes the worker is bounded too.
        // Always the three together: `--log-opt max-size` alone fails
        // `docker run` where the Engine's default driver is journald
        "--log-driver", "local", "--log-opt", "max-size=16m", "--log-opt", "max-file=2",
    ];
    if host_network {
        // a Linux Engine has no `host.docker.internal` of its own: a Store
        // URL that names it resolves to the bridge's gateway, the host; a
        // loopback URL is dialled as given, the network being the host's
        #[rustfmt::skip]
        args.extend([
            "--network", "host", "--add-host", "host.docker.internal:host-gateway",
        ]);
    } else {
        args.extend(["-p", &publish]);
    }
    #[rustfmt::skip]
    args.extend([
        "-v", &volume, "-v", &bundle_mount, "--tmpfs", &tmpfs,
        "-e", &listen, "-e", &secret_file, "-e", &log,
        "--entrypoint", "sh", image, "-c", &entry,
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
    // a host-network worker listens on the host port it was given
    let guest_port = if host_network { port } else { WORKER_PORT };
    worker::ready(spec, &guest, guest_port, address, &|| ended(spec))?;
    Ok(Running {
        endpoint: crate::spec::Endpoint { address, secret },
        lasting: spec.state,
    })
}

/// Stops the worker container; Docker itself and the Store volume stay.
pub(crate) fn stop(spec: &Spec, force: bool) -> Result<(), MachineError> {
    let Some((state, role)) = inspect(spec)? else {
        return Ok(());
    };
    if role != "machine-worker" {
        return Err(MachineError::new(
            Code::InvalidInput,
            "the recorded container is not a LayerFS worker; it is left alone",
        ));
    }
    if !matches!(state.as_str(), "running" | "restarting" | "paused") {
        return Ok(());
    }
    run(
        spec,
        &["stop", "-t", if force { "0" } else { "5" }, &spec.vm_name],
        "stopping the worker container",
    )?
    .answer("stopping the worker container")?;
    Ok(())
}

/// Loads the docker-archive `archive` (the bundle's Sandbox image) into the
/// Engine once and answers the image id Docker reports, `sha256:<64 hex>`:
/// the image of the worker container when the docker machine names none
/// (#535 P5). Its Sandboxes unpack the bundle's archive themselves. One
/// attempt; the caller records the id.
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
