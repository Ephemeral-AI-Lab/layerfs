//! Where each record lives in the home (H §7.8). What never moves nests
//! under its owner: projects under their Store, Sandboxes under their
//! machine. A sqlite Store moves between machines, so it stays at the top
//! and its record names its machine.
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

/// `machine/<machine>/sandbox/<name>/`.
pub fn sandbox_dir(home: &Path, machine: &str, sandbox: &str) -> PathBuf {
    machine_dir(home, machine).join("sandbox").join(sandbox)
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
