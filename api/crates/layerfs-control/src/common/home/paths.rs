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

/// `store/<name>/store.sqlite`: a sqlite Store's file while it is on `native`.
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

/// `step/`: the launcher-owned step entries and their idempotency index
/// (#555).
pub fn step_dir(home: &Path) -> PathBuf {
    home.join("step")
}

/// `step/launch/`: the process launcher's pid, output and exit receipts,
/// outside every launcher process (#555).
pub fn step_launch_dir(home: &Path) -> PathBuf {
    step_dir(home).join("launch")
}

/// `machine/native/run/`: the local Sandbox backends' runtime root (0755),
/// one directory per Sandbox beside the image cache the namespaces backend
/// keeps there (`images/`), in the home and under a worker's root.
pub fn run_root(root: &Path) -> PathBuf {
    machine_dir(root, "native").join("run")
}

/// `log/`: init-failures.jsonl, uncertain.jsonl.
pub fn log_dir(home: &Path) -> PathBuf {
    home.join("log")
}

/// `trace/`.
pub fn trace_dir(home: &Path) -> PathBuf {
    home.join("trace")
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
