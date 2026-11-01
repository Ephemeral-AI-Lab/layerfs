//! Store leases (H §7.8): every process that uses a sqlite Store holds
//! `store/<name>/open/<pid>`: the process that opens it, or, for a Store
//! behind a worker, the host process that sends it calls. `store move` and
//! `store remove` read them under the home lock and refuse `InUse` while one
//! is held. A lease whose process is not alive on this host is stale: it is
//! removed under the home lock and the removal is logged (O35).
use crate::home::{self, paths};
use crate::host::outcome::failed;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// This process's leases and how many holders use each.
static HELD: Mutex<BTreeMap<PathBuf, usize>> = Mutex::new(BTreeMap::new());

/// One use of a Store's lease; the last drop in this process removes it.
pub struct Lease {
    path: PathBuf,
}

impl Drop for Lease {
    fn drop(&mut self) {
        let mut held = HELD.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(count) = held.get_mut(&self.path) {
            *count -= 1;
            if *count == 0 {
                held.remove(&self.path);
                let _ = std::fs::remove_file(&self.path);
            }
        }
    }
}

/// Holds Store `store`'s lease for this process.
pub fn hold(home: &Path, store: &str) -> Result<Lease, SdkRefusal> {
    let dir = paths::lease_dir(home, store);
    let path = dir.join(std::process::id().to_string());
    let mut held = HELD.lock().unwrap_or_else(|e| e.into_inner());
    let count = held.entry(path.clone()).or_insert(0);
    if *count == 0 {
        home::private_dir(&dir)?;
        let text = format!("{}\n", home::now_ms());
        if let Err(error) = std::fs::write(&path, text) {
            held.remove(&path);
            return Err(failed(error));
        }
    }
    *held.get_mut(&path).expect("counted above") += 1;
    Ok(Lease { path })
}

/// Whether process `pid` is alive on this host.
pub fn alive(pid: u32) -> bool {
    use nix::errno::Errno;
    match nix::sys::signal::kill(nix::unistd::Pid::from_raw(pid as i32), None) {
        Ok(()) => true,
        Err(Errno::EPERM) => true,
        Err(_) => false,
    }
}

/// The processes holding Store `store`, other than this one; stale leases
/// are removed and logged (O35). The caller holds the home lock.
pub fn holders(home: &Path, store: &str) -> Result<Vec<u32>, SdkRefusal> {
    let dir = paths::lease_dir(home, store);
    let mut live = Vec::new();
    let entries = match std::fs::read_dir(&dir) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(live),
        Err(e) => return Err(failed(e)),
        Ok(entries) => entries,
    };
    for entry in entries {
        let entry = entry.map_err(failed)?;
        let Some(pid) = entry
            .file_name()
            .to_str()
            .and_then(|n| n.parse::<u32>().ok())
        else {
            continue;
        };
        if pid == std::process::id() {
            continue;
        }
        if alive(pid) {
            live.push(pid);
        } else {
            std::fs::remove_file(entry.path()).map_err(failed)?;
            home::log_line(
                home,
                "leases.jsonl",
                &SdkValue::map([
                    ("at", SdkValue::Int(home::now_ms())),
                    ("store", SdkValue::text(store)),
                    ("removed_stale_lease_of", SdkValue::Int(i64::from(pid))),
                ]),
            )?;
        }
    }
    live.sort_unstable();
    Ok(live)
}
