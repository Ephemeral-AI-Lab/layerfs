//! The host checks (#551 B10-171…176): the Docker socket, a VM provider's
//! tool, the Sandbox image, the mount driver, free disk and a machine's
//! worker and lease. Each reads records, a socket or a tool's version and
//! reports; none starts a machine or a Sandbox, and none writes the home.
use super::report::Findings;
use super::scan::Scanned;
use crate::common::home::paths;
use std::path::{Path, PathBuf};

/// Below this, a home build can fail; warn (#551 B10-175).
const MIN_FREE: u64 = 1 << 30;

/// Runs the host checks on `home`.
pub(super) fn check(home: &Path, scanned: &Scanned, findings: &mut Findings) {
    disk(home, findings);
    docker(scanned, findings);
    images(scanned, findings);
    drivers(scanned, findings);
    providers(home, scanned, findings);
    workers(home, scanned, findings);
}

/// #551 B10-175: free disk where the home and its Stores are.
fn disk(home: &Path, findings: &mut Findings) {
    let shown = home.display().to_string();
    match nix::sys::statvfs::statvfs(home) {
        Ok(stat) => {
            // `blocks_available` is `u32` on macOS and `u64` on Linux; the
            // widening conversion is explicit (as in the Store headroom
            // checks) so neither platform's clippy flags a useless one.
            let free = u128::from(stat.blocks_available()) * u128::from(stat.fragment_size());
            if free < u128::from(MIN_FREE) {
                findings.warning(
                    ("home", &shown),
                    format!(
                        "{} has {} MiB free; a build, a VM image or a Store copy can fail below 1 GiB",
                        home.display(),
                        free / (1 << 20)
                    ),
                    "free disk space (`python3 tools/cleanup.py check` lists the caches)".to_owned(),
                );
            }
        }
        Err(error) => findings.warning(
            ("home", &shown),
            format!("{}'s free disk cannot be read: {error}", home.display()),
            "check that the home's filesystem is mounted".to_owned(),
        ),
    }
}

/// #551 B10-171: a declared Docker socket must be there, and any socket that
/// is there must accept a connection. A missing default socket on a host
/// without Docker is not a finding: the docker machine is simply absent.
fn docker(scanned: &Scanned, findings: &mut Findings) {
    for machine in &scanned.machines {
        if machine.platform_name() != Some("docker") {
            continue;
        }
        let declared = machine.docker().map(PathBuf::from);
        let socket = declared
            .clone()
            .unwrap_or_else(|| PathBuf::from("/var/run/docker.sock"));
        let name = machine.name();
        let entity = ("machine", name);
        match std::os::unix::net::UnixStream::connect(&socket) {
            Ok(_) => {}
            Err(error) if declared.is_some() => findings.error(
                entity,
                format!(
                    "machine {name}'s Docker socket {} cannot be reached: {error}",
                    socket.display()
                ),
                format!(
                    "`layerfs machine update {name} --docker <socket>` names the Engine's socket, or start Docker"
                ),
            ),
            Err(error) if socket.exists() => findings.warning(
                entity,
                format!(
                    "machine {name}'s Docker socket {} exists but does not answer: {error}",
                    socket.display()
                ),
                "start Docker, then `layerfs machine start docker`".to_owned(),
            ),
            Err(_) => {}
        }
    }
}

/// #551 B10-173: a Sandbox image is a file that must exist, or a digest the
/// docker machine must record.
fn images(scanned: &Scanned, findings: &mut Findings) {
    for sandbox in &scanned.sandboxes {
        let Some(image) = sandbox.image() else {
            continue;
        };
        let name = sandbox.name();
        let entity = ("sandbox", name);
        if image.starts_with('/') {
            if !Path::new(image).is_file() {
                findings.error(
                    entity,
                    format!("Sandbox {name}'s image file {image} is not there"),
                    format!(
                        "recreate it on a machine whose image exists: `layerfs sandbox close {name}`, then `layerfs sandbox.create <project> {name} <machine>`"
                    ),
                );
            }
            continue;
        }
        if sandbox.machine() != "docker" {
            continue;
        }
        let recorded = (scanned.machines.iter())
            .find(|machine| machine.name() == sandbox.machine())
            .and_then(|machine| {
                machine
                    .image()
                    .map(str::to_owned)
                    .or_else(|| machine.loaded_image().map(|(id, _)| id.to_owned()))
            });
        if recorded.as_deref() != Some(image) {
            findings.warning(
                entity,
                format!(
                    "Sandbox {name} runs image {image}, which the docker machine's record does not name"
                ),
                "`layerfs machine update docker --image sha256:<digest>` records it, or recreate the Sandbox on an image the machine names"
                    .to_owned(),
            );
        }
    }
}

/// #551 B10-174: the mount driver a recorded process Sandbox needs on this
/// host: FUSE and its device on Linux, macFUSE's mount helper on macOS.
fn drivers(scanned: &Scanned, findings: &mut Findings) {
    let uses_process = (scanned.sandboxes.iter()).any(|sandbox| {
        sandbox.effective_isolation() == Some(layerfs_sandbox::IsolationClass::Process)
    });
    if !uses_process {
        return;
    }
    let mut missing = process_missing();
    if cfg!(target_os = "macos") && !on_path("mount_fusefs") {
        missing.push("host:mount_fusefs".to_owned());
    }
    if missing.is_empty() {
        return;
    }
    let shown = "a Sandbox runs behind process isolation";
    findings.warning(
        ("home", shown),
        format!(
            "the mount driver a process Sandbox needs is missing: {}",
            missing.join(", ")
        ),
        "install FUSE (Linux `/dev/fuse`, macOS macFUSE), then run `layerfs machine platforms`"
            .to_owned(),
    );
}

/// What this host lacks for a process backend (#551 B10-174), from the
/// machine layer's probe: no tool is run.
#[cfg(feature = "machine")]
fn process_missing() -> Vec<String> {
    layerfs_machine::native_missing(layerfs_machine::HostOs::current())
}

#[cfg(not(feature = "machine"))]
fn process_missing() -> Vec<String> {
    Vec::new()
}

fn on_path(binary: &str) -> bool {
    std::env::var_os("PATH")
        .map(|path| std::env::split_paths(&path).any(|dir| dir.join(binary).is_file()))
        .unwrap_or(false)
}

/// #551 B10-172: the provider tool of a VM machine a Sandbox runs on. A
/// machine no Sandbox uses is the support table's business
/// (`machine.platforms`); this check reports only what a call will meet.
#[cfg(feature = "machine")]
fn providers(home: &Path, scanned: &Scanned, findings: &mut Findings) {
    let Ok(tools) = crate::machine::machine_tools(home) else {
        return;
    };
    for machine in &scanned.machines {
        let name = machine.name();
        if !(scanned.sandboxes.iter()).any(|sandbox| sandbox.machine() == name) {
            continue;
        }
        let Ok(kind) = machine.platform() else {
            continue;
        };
        if !kind.created() {
            continue;
        }
        let probe = layerfs_machine::probe(kind, &tools);
        if probe.ready {
            continue;
        }
        findings.warning(
            ("machine", name),
            format!(
                "machine {name}'s {} provider is not available on this host: {}",
                kind.name(),
                probe.missing.join(", ")
            ),
            "install the provider's tool, or move the Sandbox off it: `layerfs sandbox close <name>`, then `layerfs sandbox.create <project> <name> <machine>`"
                .to_owned(),
        );
    }
}

#[cfg(not(feature = "machine"))]
fn providers(_home: &Path, _scanned: &Scanned, _findings: &mut Findings) {}

/// #551 B10-176: a machine's worker record and lease: a stopped or failed
/// machine left a worker record, a running worker machine has none, or a
/// recorded forward process is gone. It never dials the worker; the hello
/// check is `workers.rs`.
fn workers(home: &Path, scanned: &Scanned, findings: &mut Findings) {
    for machine in &scanned.machines {
        let name = machine.name();
        let entity = ("machine", name);
        let recorded = crate::common::home::read_record(&paths::worker_record(home, name))
            .ok()
            .flatten();
        let state = machine.recorded_state().unwrap_or("");
        if recorded.is_some() && matches!(state, "stopped" | "failed") {
            findings.warning(
                entity,
                format!(
                    "machine {name} is {state}, but its worker record remains; the next start replaces it"
                ),
                format!("`layerfs machine start {name}`, or `layerfs machine stop {name}` to stop it"),
            );
        }
        if recorded.is_none() && matches!(state, "running" | "ready") && runs_worker(machine) {
            findings.warning(
                entity,
                format!(
                    "machine {name} is recorded {state}, but it has no worker record; calls that need its worker are refused"
                ),
                format!("`layerfs machine start {name}` starts its worker"),
            );
        }
        let forward = recorded
            .as_ref()
            .and_then(|record| record.get_int("forward_pid"))
            .filter(|pid| *pid > 0);
        if let Some(pid) = forward {
            let alive =
                nix::sys::signal::kill(nix::unistd::Pid::from_raw(pid as i32), None).is_ok();
            if !alive {
                findings.warning(
                    entity,
                    format!(
                        "machine {name}'s recorded forward process {pid} is gone; the worker is reached only through a new start"
                    ),
                    format!("`layerfs machine stop {name}`, then `layerfs machine start {name}`"),
                );
            }
        }
    }
}

/// Whether a machine's calls run in a worker (#551 B10-176): every platform
/// but `native` does. A provider name in `doctor` is allowed by #551 §6; the
/// routing itself stays in `layerfs-machine`'s profile table.
fn runs_worker(machine: &crate::machine::record::MachineRecord) -> bool {
    matches!(
        machine.platform_name(),
        Some("docker" | "ssh" | "lima" | "firecracker" | "microsandbox" | "apple-container")
    )
}
