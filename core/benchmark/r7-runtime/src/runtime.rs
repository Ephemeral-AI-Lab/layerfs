use crate::{args::Args, events::Events, Result};
use layerfs_bridge::{daemon_setup::{DaemonLimits, DaemonSetup}, native, provision::StoreManifest};
use layerfs_sandbox::{backend::docker::{Docker, ExecRequest}, CommandIdentity};
use layerfs_sdk::{ManagedSandbox, SandboxApi, SandboxCreate};
use std::{fs, path::Path, time::{Duration, Instant}};

pub fn start(args: &Args, manifest: Option<StoreManifest>, events: &mut Events) -> Result<ManagedSandbox> {
    let controller = native::generate_keypair()?;
    let daemon = native::generate_keypair()?;
    let setup = DaemonSetup {
        listen: "0.0.0.0:30421".into(),
        private_key: daemon.private,
        control_peer: controller.public,
        store: "/layerfs-store/global/store.sqlite".into(),
        overlay: "/layerfs-local/overlay/overlay.sqlite".into(),
        mounts: "/workspaces".into(),
        command_uid: args.uid,
        command_gid: args.gid,
        // OwnerConfig::default() and the current S8 immutable-cache allowance,
        // frozen here. They are never enlarged in response to a measurement.
        limits: DaemonLimits {
            connections: 4,
            handshake_ms: 2000,
            read_handles: 2,
            cache_bytes: 8 * 1024 * 1024,
            owner_bytes: 8 * 1024 * 1024,
            lifecycle_reserve: 64 * 1024,
            namespaces: 16,
            ordinary_jobs: 16,
            lifecycle_jobs: 2,
            pager_kib: 1024,
        },
        existing_store: manifest,
    };
    let begun = Instant::now();
    let owner = SandboxApi::new(Docker::new(&args.socket, Duration::from_secs(3))?)
        .create(SandboxCreate {
            deployment: layerfs_sandbox::backend::docker::SandboxRequest {
                image: args.image.clone(), store_volume: args.volume.clone(), port: 30421,
            },
            setup,
            executable: args.executable.clone(),
            controller_private: controller.private,
            daemon_peer: daemon.public,
            startup_wait: Duration::from_secs(3),
        }).map_err(|error| format!("sandbox original failure {error:?}"))?;
    let elapsed = events.span(begun);
    events.phase("daemon_setup", Some(elapsed), &format!("container={} startup={:?} handshake={:?}", owner.runtime.identity(), owner.startup, owner.handshake))?;
    Ok(owner)
}

/// An external ordinary runtime Bash: no WorkspaceApi.exec and no registration.
/// Streams go directly to files, with no output-sized resident collection.
pub fn command(owner: &ManagedSandbox, body: &str, directory: &str, label: &str, observer: bool, events: &mut Events) -> Result<()> {
    let (stdout_path, mut stdout) = events.output(label, "stdout")?;
    let (stderr_path, mut stderr) = events.output(label, "stderr")?;
    let arguments = vec!["/bin/bash".into(), "-o".into(), "pipefail".into(), "-c".into(), body.into()];
    let begun = Instant::now();
    let created = if observer {
        owner.runtime.create_exec(ExecRequest {
            identity: CommandIdentity { uid: 0, gid: 0 }, arguments,
            environment: vec![], directory: directory.into(), stdin: false,
        })
    } else {
        owner.exec(arguments, vec!["LAYERFS_CONSTRUCTION_WORKERS=1".into()], directory.into(), false)
    }.map_err(|error| format!("external exec create {error:?}"))?;
    let attached = created.start().map_err(|error| format!("external exec start {error:?}"))?;
    let (_input, output, handle) = attached.into_parts().map_err(|error| format!("external exec streams {error:?}"))?;
    output.copy_to(&mut stdout, &mut stderr).map_err(|error| format!("external stream disposal {error:?}"))?;
    let streams_ns = begun.elapsed().as_nanos();
    let inspected = handle.inspect().map_err(|error| format!("external status {error:?}"))?;
    let elapsed = events.span(begun);
    events.phase(label, Some(elapsed), &format!("streams_eof_observed_ns={streams_ns} status_observed_ns={} inspection={inspected:?} stdout={} stderr={} registered_execs=0", elapsed.duration_ns, stdout_path.display(), stderr_path.display()))?;
    if inspected.known_root_exit() != Some(0) {
        return Err(format!("original external command exit: {inspected:?}").into());
    }
    Ok(())
}

pub fn body(path: &Path) -> Result<String> {
    Ok(fs::read_to_string(path)?)
}

pub fn snapshot(owner: &ManagedSandbox, events: &mut Events) -> Result<()> {
    // Observer-only root identity reads the daemon's protected files. It
    // acquires no filesystem owner and makes no mutation or cache hint.
    command(owner,
        "set -e; printf 'DAEMON_STATUS\\n'; cat /proc/1/status; printf 'BACKING_BYTES\\n'; for p in /layerfs-store/global/store.sqlite /layerfs-store/global/store.sqlite-wal /layerfs-store/global/store.sqlite-shm /layerfs-local/overlay/overlay.sqlite /layerfs-local/overlay/overlay.sqlite-journal; do if test -e \"$p\"; then stat -c '%n %s %b %B' \"$p\"; else printf '%s ABSENT\\n' \"$p\"; fi; done",
        "/", "external_snapshot", true, events)
}
