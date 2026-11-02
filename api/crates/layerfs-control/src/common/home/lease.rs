//! Store leases (H §7.8): every process that uses a sqlite Store holds
//! `store/<name>/open/<pid>`: the process that opens it, or, for a Store
//! behind a worker, the host process that sends it calls. A lease is taken
//! under the home lock after the Store's record shows no move or remove
//! running (`in_progress`), so a lease and a move never overlap. `store
//! move` and `store remove` read leases under the home lock and refuse
//! `InUse` while one is held. A lease names its process by id, kernel start time and boot, so
//! a reused id does not keep it alive; a lease whose process is not alive on
//! this host is stale: it is removed under the home lock and the removal is
//! logged (O35).
use crate::common::home::{self, paths, read_record};
use crate::common::outcome::{failed, refused};
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_sandbox::local::LocalProcess;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

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

/// This process as leases name it, read once; `None` where the kernel does
/// not say (its leases are then matched by id alone).
fn me() -> Option<(u64, u128)> {
    static ME: OnceLock<Option<(u64, u128)>> = OnceLock::new();
    *ME.get_or_init(|| {
        let process = LocalProcess::observe(std::process::id()).ok()?;
        Some((process.start, process.boot?))
    })
}

/// The fields that name this process in a record (a move's `in_progress`).
pub(crate) fn identity() -> Vec<(&'static str, SdkValue)> {
    let mut fields = vec![("pid", SdkValue::Int(i64::from(std::process::id())))];
    if let Some((start, boot)) = me() {
        fields.push(("pid_start", SdkValue::text(start.to_string())));
        fields.push(("pid_boot", SdkValue::text(format!("{boot:032x}"))));
    }
    fields
}

/// Whether the process a record's [`identity`] fields name is alive.
pub(crate) fn recorded_alive(value: &SdkValue) -> bool {
    let Some(pid) = value.get_int("pid").and_then(|p| u32::try_from(p).ok()) else {
        return false;
    };
    alive_as(
        pid,
        value.get_text("pid_start").and_then(|s| s.parse().ok()),
        value
            .get_text("pid_boot")
            .and_then(|b| u128::from_str_radix(b, 16).ok()),
    )
}

/// Holds Store `store`'s lease for this process. Under the home lock it
/// re-reads the Store's record: no record is `NoSuchStore`, a move or remove
/// running (by any live process, this one included) is `InUse`.
pub fn hold(home: &Path, store: &str) -> Result<Lease, SdkRefusal> {
    claim(home, store).map(|(lease, _)| lease)
}

/// [`hold`], answering the Store's record as read under the home lock.
pub(crate) fn claim(home: &Path, store: &str) -> Result<(Lease, SdkValue), SdkRefusal> {
    let _lock = home::lock::take(home)?;
    let record = read_record(&paths::store_record(home, store))?
        .ok_or_else(|| refused("NoSuchStore", format!("no Store named {store}")))?;
    crate::store::route::moving(&record, store)?;
    Ok((hold_locked(home, store)?, record))
}

/// Holds Store `store`'s lease under `root` with no record check: the
/// caller holds the home lock, or is a worker under its own root.
pub(crate) fn hold_locked(root: &Path, store: &str) -> Result<Lease, SdkRefusal> {
    let dir = paths::lease_dir(root, store);
    let path = dir.join(std::process::id().to_string());
    let mut held = HELD.lock().unwrap_or_else(|e| e.into_inner());
    let count = held.entry(path.clone()).or_insert(0);
    if *count == 0 {
        home::private_dir(&dir)?;
        let mut text = home::now_ms().to_string();
        if let Some((start, boot)) = me() {
            text.push_str(&format!(" {start} {boot:032x}"));
        }
        text.push('\n');
        if let Err(error) = std::fs::write(&path, text) {
            held.remove(&path);
            return Err(failed(error));
        }
    }
    *held.get_mut(&path).expect("counted above") += 1;
    Ok(Lease { path })
}

/// Whether this process holds Store `store`'s lease under `root` now.
pub(crate) fn held_here(root: &Path, store: &str) -> bool {
    let path = paths::lease_dir(root, store).join(std::process::id().to_string());
    HELD.lock()
        .unwrap_or_else(|e| e.into_inner())
        .contains_key(&path)
}

/// Whether process `pid` is alive on this host. Ids 0 and above `i32::MAX`
/// name no single process (`kill` would read them as a group): never alive.
pub fn alive(pid: u32) -> bool {
    use nix::errno::Errno;
    let id = match i32::try_from(pid) {
        Ok(id) if id > 0 => id,
        _ => return false,
    };
    match nix::sys::signal::kill(nix::unistd::Pid::from_raw(id), None) {
        Ok(()) => true,
        Err(Errno::EPERM) => true,
        Err(_) => false,
    }
}

/// Whether `pid` is alive and, when its start time and boot were recorded,
/// still the same process.
fn alive_as(pid: u32, start: Option<u64>, boot: Option<u128>) -> bool {
    if !alive(pid) {
        return false;
    }
    match (start, boot) {
        (Some(start), Some(boot)) => LocalProcess {
            pid,
            start,
            boot: Some(boot),
        }
        .alive(),
        _ => true,
    }
}

/// Whether lease text `text` (`<at> [<start> <boot>]`) of `pid` names a
/// live process.
fn lease_alive(pid: u32, text: &str) -> bool {
    let mut fields = text.split_ascii_whitespace().skip(1);
    let start = fields.next().and_then(|s| s.parse().ok());
    let boot = fields.next().and_then(|b| u128::from_str_radix(b, 16).ok());
    alive_as(pid, start, boot)
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
        let text = match std::fs::read_to_string(entry.path()) {
            Ok(text) => text,
            // its holder dropped it meanwhile
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err(failed(e)),
        };
        if lease_alive(pid, &text) {
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
