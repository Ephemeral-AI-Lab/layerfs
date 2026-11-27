//! Where each record lives in the home (layout 3, #536 §5). Each entity
//! kind has one folder and each entity one identity, its name: projects
//! nest under their Store, Workspaces under their Sandbox. Sandboxes and
//! sqlite Stores are at the top and their records name their machine. A
//! worker's root uses the same paths for what it keeps (§6).
use std::path::{Path, PathBuf};

/// `store/<name>/`.
pub fn store_dir(home: &Path, store: &str) -> PathBuf {
    home.join("store").join(store)
}

/// `store/<name>/record.json`: kind, machine, backups.
pub fn store_record(home: &Path, store: &str) -> PathBuf {
    store_dir(home, store).join("record.json")
}

/// `store/<name>/store.sqlite`: a sqlite Store's file while it is on the machine whose
/// calls run in this process.
pub fn store_file(home: &Path, store: &str) -> PathBuf {
    store_dir(home, store).join("store.sqlite")
}

/// `store/<name>/open/`: the leases of processes that hold it open.
pub fn lease_dir(home: &Path, store: &str) -> PathBuf {
    store_dir(home, store).join("open")
}

/// `store/<store>/project/<project>/`.
pub fn project_dir(home: &Path, store: &str, project: &str) -> PathBuf {
    store_dir(home, store).join("project").join(project)
}

/// `store/<store>/project/<project>/record.json`.
pub fn project_record(home: &Path, store: &str, project: &str) -> PathBuf {
    project_dir(home, store, project).join("record.json")
}

/// `backup/<bucket>/record.json`: provider, url, prefix, region, key reference.
pub fn bucket_record(home: &Path, bucket: &str) -> PathBuf {
    home.join("backup").join(bucket).join("record.json")
}

/// `machine/<name>/`.
pub fn machine_dir(home: &Path, machine: &str) -> PathBuf {
    home.join("machine").join(machine)
}

/// `machine/<name>/record.json`: kind, size, lasting state.
pub fn machine_record(home: &Path, machine: &str) -> PathBuf {
    machine_dir(home, machine).join("record.json")
}

/// `machine/<name>/worker.json`: endpoint, secret, live forward (0600).
pub fn worker_record(home: &Path, machine: &str) -> PathBuf {
    machine_dir(home, machine).join("worker.json")
}

/// The native worker's ordinary endpoint.
pub fn host_worker_socket(home: &Path) -> PathBuf {
    machine_dir(home, crate::names::HOST_MACHINE).join("worker.sock")
}

/// The native worker's endpoint for streamed file writes, with admission
/// of its own: a held write never takes a place of the ordinary endpoint.
pub fn host_worker_writes(home: &Path) -> PathBuf {
    machine_dir(home, crate::names::HOST_MACHINE).join("writes.sock")
}

/// The native worker's management endpoint, with independent admission.
pub fn host_worker_control(home: &Path) -> PathBuf {
    machine_dir(home, crate::names::HOST_MACHINE).join("worker.ctl")
}

/// Stable lock held by the worker until its endpoints have been removed.
pub fn host_worker_lock(home: &Path) -> PathBuf {
    machine_dir(home, crate::names::HOST_MACHINE).join("worker.lock")
}

/// Serializes launch attempts without relinquishing the worker's lifetime lock.
pub fn host_worker_startup_lock(home: &Path) -> PathBuf {
    machine_dir(home, crate::names::HOST_MACHINE).join("worker-start.lock")
}

/// The native worker's last failed background start (#833): what failed,
/// written by the caller that started it or by the worker before its
/// endpoints were published, and removed by the next worker that publishes
/// them.
pub fn host_worker_start_failure(home: &Path) -> PathBuf {
    machine_dir(home, crate::names::HOST_MACHINE).join("worker-start-failed.json")
}

/// The native worker's background start in progress (#833): written by
/// the caller before it spawns the worker, under the startup lock the
/// worker then holds, and removed when the worker publishes its endpoints.
/// Found with that lock free and no worker, the worker ended first.
pub fn host_worker_start_pending(home: &Path) -> PathBuf {
    machine_dir(home, crate::names::HOST_MACHINE).join("worker-starting.json")
}

/// `machine/<machine>/sandbox-retired/`: one tombstone per closed Sandbox,
/// outside `sandbox/` so a Sandbox listing never mistakes it for one.
pub fn sandbox_retired_dir(home: &Path, machine: &str) -> PathBuf {
    machine_dir(home, machine).join("sandbox-retired")
}

/// `machine/<machine>/sandbox-retired/<name>.json`: a closed Sandbox's
/// name, generation and retired time, written before its record is removed.
pub fn sandbox_tombstone(home: &Path, machine: &str, sandbox: &str) -> PathBuf {
    sandbox_retired_dir(home, machine).join(format!("{sandbox}.json"))
}

/// `sandbox/<name>/`: a Sandbox's record, daemon keys and Workspaces.
pub fn sandbox_dir(home: &Path, sandbox: &str) -> PathBuf {
    home.join("sandbox").join(sandbox)
}

/// `sandbox/<name>/record.json`.
pub fn sandbox_record(home: &Path, sandbox: &str) -> PathBuf {
    sandbox_dir(home, sandbox).join("record.json")
}

/// `sandbox/<name>/control.json`: its daemon's keys (0600, #535 G4).
pub fn sandbox_control(home: &Path, sandbox: &str) -> PathBuf {
    sandbox_dir(home, sandbox).join("control.json")
}

/// `sandbox/<name>/workspace/`: one `<id>.json` per Workspace.
pub fn workspace_dir(home: &Path, sandbox: &str) -> PathBuf {
    sandbox_dir(home, sandbox).join("workspace")
}

/// The folder under `machine/` that names a worker root's run root: every
/// worker root (docker, lima, apple-container) is a Linux guest.
pub const GUEST_RUN: &str = "linux-guest";

/// The local Sandbox backends' runtime root (0755) of side root `root`, a
/// home or a worker's root: one directory per Sandbox beside the image
/// cache the namespaces backend keeps there (`images/`). It is named for
/// the side's kernel and role (#819 R): `machine/linux-guest/run/` in a
/// worker's root ([`crate::home::guest::is_root`], which the worker marks
/// before it sweeps or serves anything), else the host machine's own
/// folder, `machine/macos-host/run/` or `machine/linux-host/run/`. The
/// earlier `machine/native/run/` is not read.
pub fn run_root(root: &Path) -> PathBuf {
    let folder = match crate::home::guest::is_root(root) {
        true => GUEST_RUN,
        false => crate::names::HOST_MACHINE,
    };
    machine_dir(root, folder).join("run")
}

/// `log/`: `uncertain.jsonl` (never rotated) and the logs that rotate
/// once to `<name>.1.jsonl`: `leases`, `sweep` and, on the side that keeps
/// a Sandbox's command records, `exec`.
pub fn log_dir(home: &Path) -> PathBuf {
    home.join("log")
}

/// `log/rotate.lock`: a rotation of a `log/*.jsonl` file takes it, as
/// `trace/calls.lock` (layerfs-project#180).
pub fn log_rotate_lock(home: &Path) -> PathBuf {
    log_dir(home).join("rotate.lock")
}

/// `log/worker.log` under a worker's root: the worker's output on every
/// machine kind, `/layerfs/data/log/worker.log` in the machine (0600). Its
/// launcher appends to it and rotates it once to `worker.1.log` when a
/// start finds it past 16 MiB (layerfs-project#178 step 0).
pub fn worker_log(root: &Path) -> PathBuf {
    log_dir(root).join("worker.log")
}

/// `trace/`.
pub fn trace_dir(home: &Path) -> PathBuf {
    home.join("trace")
}

/// `trace/calls.jsonl`: the call log, one line per call (#659).
pub fn calls_file(home: &Path) -> PathBuf {
    trace_dir(home).join("calls.jsonl")
}

/// `trace/calls.1.jsonl`: the call log's one rotated file (#659).
pub fn calls_rotated(home: &Path) -> PathBuf {
    trace_dir(home).join("calls.1.jsonl")
}

/// `trace/calls.lock`: rotation and reset of the call log take it (#659).
pub fn calls_lock(home: &Path) -> PathBuf {
    trace_dir(home).join("calls.lock")
}

/// The names of the folders under `dir` (sorted); none when it is absent.
pub fn names(dir: &Path) -> std::io::Result<Vec<String>> {
    let mut names = Vec::new();
    match std::fs::read_dir(dir) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(names),
        Err(error) => return Err(error),
        Ok(entries) => {
            for entry in entries {
                let entry = entry?;
                if entry.file_type()?.is_dir() {
                    if let Some(name) = entry.file_name().to_str() {
                        if !name.starts_with('.') {
                            names.push(name.to_owned());
                        }
                    }
                }
            }
        }
    }
    names.sort();
    Ok(names)
}
