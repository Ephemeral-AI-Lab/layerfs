//! Store leases (H §7.8): every process that uses a Store holds
//! `store/<name>/open/<pid>`: the process that opens it (a PostgreSQL
//! Store's too), or, for a Store behind a worker, the host process that
//! sends it calls. A lease is taken under the Store's lock (#536 §8) after
//! the Store's record shows no operation holding it (`op`), so a lease and
//! a move or remove never overlap. `store move`, `store remove` and
//! `store disconnect` read leases under the Store's lock and refuse `InUse` while
//! one is held. A lease names its process by id, kernel start time and
//! boot, so a reused id does not keep it alive; a lease whose process is
//! not alive on this host is stale: a move, remove or disconnect, and
//! a process taking its own lease on the Store ([`claim`]), removes it under
//! the Store's lock and logs the removal (O35), so a process that ended
//! without closing its Stores leaves nothing once the Store is used again;
//! reads (`store.get`, `store.list`, the doctors) ignore it and
//! change nothing.
use crate::record::StoreRecord;
use layerfs_home::home::{self, paths};
use layerfs_home::outcome::{failed, refused};
use layerfs_home::process::{alive_as, me};
use layerfs_protocol::sdk::{SdkRefusal, SdkValue};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// This process's leases and how many holders use each.
static HELD: Mutex<BTreeMap<PathBuf, usize>> = Mutex::new(BTreeMap::new());
const HOST_WORKER: &str = "host-worker";

/// One use of a Store's lease; the last drop in this process removes it.
pub struct Lease {
    path: Option<PathBuf>,
}

impl Lease {
    /// Releases this use and reports failure to remove the last lease file.
    /// A file that is already absent is settled.
    pub fn release(mut self) -> Result<(), SdkRefusal> {
        self.release_file()
    }

    fn release_file(&mut self) -> Result<(), SdkRefusal> {
        let Some(path) = self.path.take() else {
            return Ok(());
        };
        let mut held = HELD.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(count) = held.get_mut(&path) {
            *count -= 1;
            if *count == 0 {
                held.remove(&path);
                remove_file(&path)?;
            }
        }
        Ok(())
    }
}

impl Drop for Lease {
    fn drop(&mut self) {
        let _ = self.release_file();
    }
}

/// Holds Store `store`'s lease for this process. Under the Store's lock it
/// re-reads the Store's record: no record is `NoSuchStore`; an operation
/// running on it (by any live process, this one included) is `Busy`, one a
/// gone process left unresolved `Unresolved`. Stale leases of the Store are
/// removed there first ([`holders`]).
pub fn hold(home: &Path, store: &str) -> Result<Lease, SdkRefusal> {
    claim(home, store).map(|(lease, _)| lease)
}

/// [`hold`], answering the Store's record as read under the Store's lock.
pub fn claim(home: &Path, store: &str) -> Result<(Lease, StoreRecord), SdkRefusal> {
    let none = || refused("NoSuchStore", format!("no Store named {store}"));
    if !paths::store_record(home, store).is_file() {
        return Err(none());
    }
    let _store = crate::record::lock(home, store)?;
    let record = StoreRecord::read(home, store)?.ok_or_else(none)?;
    record.unblocked()?;
    record.admit_machine(home)?;
    Ok((claim_unblocked_locked(home, store)?, record))
}

/// Claims a lease after the caller checked the record under the Store lock.
pub(crate) fn claim_unblocked_locked(home: &Path, store: &str) -> Result<Lease, SdkRefusal> {
    // the leases of processes that ended go now; the live holders' stay
    holders(home, store)?;
    hold_locked(home, store)
}

/// Holds Store `store`'s lease under `root` with no record check: the
/// caller holds the Store's lock, or is a worker under its own root.
pub fn hold_locked(root: &Path, store: &str) -> Result<Lease, SdkRefusal> {
    hold_role_locked(root, store, None)
}

/// Acquires a native worker cache lease while holding the Store record lock.
pub(crate) fn hold_host_locked(root: &Path, store: &str) -> Result<Lease, SdkRefusal> {
    hold_role_locked(root, store, Some(HOST_WORKER))
}

fn hold_role_locked(root: &Path, store: &str, role: Option<&str>) -> Result<Lease, SdkRefusal> {
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
        if let Some(role) = role {
            text.push(' ');
            text.push_str(role);
        }
        text.push('\n');
        if let Err(error) = std::fs::write(&path, text) {
            held.remove(&path);
            return Err(failed(error));
        }
    }
    *held.get_mut(&path).expect("counted above") += 1;
    Ok(Lease { path: Some(path) })
}

/// Whether this process holds Store `store`'s lease under `root` now.
pub fn held_here(root: &Path, store: &str) -> bool {
    let path = paths::lease_dir(root, store).join(std::process::id().to_string());
    HELD.lock()
        .unwrap_or_else(|e| e.into_inner())
        .contains_key(&path)
}

/// A process identity that can be matched to a lease without PID reuse.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Owner {
    pid: u32,
    start: u64,
    boot: u128,
}

impl Owner {
    /// Reads the three identity fields of an authenticated process.
    pub fn decode(value: &SdkValue) -> Option<Self> {
        Some(Self {
            pid: u32::try_from(value.get_int("pid")?).ok()?,
            start: value.get_text("pid_start")?.parse().ok()?,
            boot: u128::from_str_radix(value.get_text("pid_boot")?, 16).ok()?,
        })
    }

    /// Identity stored with a maintenance intent.
    pub fn to_value(&self) -> SdkValue {
        SdkValue::map([
            ("pid", SdkValue::Int(i64::from(self.pid))),
            ("pid_start", SdkValue::text(self.start.to_string())),
            ("pid_boot", SdkValue::text(format!("{:032x}", self.boot))),
        ])
    }

    /// The process number shown in holder refusals.
    pub fn pid(&self) -> u32 {
        self.pid
    }

    /// Whether this is the current process with its exact identity.
    pub fn mine(&self) -> bool {
        self.pid == std::process::id() && me() == Some((self.start, self.boot))
    }

    /// Read-only observation of this exact live lease. A caller making an
    /// admission decision must already hold the Store lock.
    pub fn holds(&self, home: &Path, store: &str) -> Result<bool, SdkRefusal> {
        Ok(leases(home, store)?
            .iter()
            .any(|lease| lease.alive && lease.owner.as_ref() == Some(self)))
    }

    /// Whether the exact previous owner released its lease or is proved dead.
    /// A live PID with an incomplete identity still blocks settlement.
    pub fn released(&self, home: &Path, store: &str) -> Result<bool, SdkRefusal> {
        Ok(!leases(home, store)?.iter().any(|lease| {
            lease.alive
                && lease.pid == self.pid
                && lease.owner.as_ref().is_none_or(|owner| owner == self)
        }))
    }
}

struct LeaseFile {
    pid: u32,
    path: PathBuf,
    alive: bool,
    owner: Option<Owner>,
    host_worker: bool,
}

/// Store `store`'s leases other than this process's. Read only.
fn leases(home: &Path, store: &str) -> Result<Vec<LeaseFile>, SdkRefusal> {
    let dir = paths::lease_dir(home, store);
    let mut found = Vec::new();
    let entries = match std::fs::read_dir(&dir) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(found),
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
        let path = entry.path();
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            // its holder dropped it meanwhile
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err(failed(e)),
        };
        let mut fields = text.split_ascii_whitespace().skip(1);
        let start = fields.next().and_then(|s| s.parse().ok());
        let boot = fields.next().and_then(|b| u128::from_str_radix(b, 16).ok());
        let owner = start
            .zip(boot)
            .map(|(start, boot)| Owner { pid, start, boot });
        found.push(LeaseFile {
            pid,
            path,
            alive: alive_as(pid, start, boot),
            owner,
            host_worker: text.split_ascii_whitespace().next_back() == Some(HOST_WORKER),
        });
    }
    found.sort_unstable_by_key(|lease| lease.pid);
    Ok(found)
}

/// Live host worker leases whose exact owner was not authenticated. Caller
/// holds the Store lock, so a freshly acquired host lease is visible here.
pub(crate) fn unmatched_host_workers(
    home: &Path,
    store: &str,
    verified: Option<&Owner>,
) -> Result<Vec<u32>, SdkRefusal> {
    Ok(leases(home, store)?
        .into_iter()
        .filter(|lease| {
            lease.alive
                && lease.host_worker
                && !verified.is_some_and(|owner| lease.owner.as_ref() == Some(owner))
        })
        .map(|lease| lease.pid)
        .collect())
}

/// Removes the lease file at `path`: `false` when it is gone already. A
/// holder drops its own lease without the Store's lock ([`Lease`]'s drop),
/// so a lease read here can be gone by the time its holder is found ended:
/// the holder dropped it and then ended, and nothing stale is left.
fn remove_file(path: &Path) -> Result<bool, SdkRefusal> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(true),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(failed(e)),
    }
}

/// The processes holding Store `store`, other than this one; stale leases
/// are removed and logged (O35). The caller holds the Store's lock.
pub fn holders(home: &Path, store: &str) -> Result<Vec<u32>, SdkRefusal> {
    holders_except(home, store, None)
}

/// Like `holders`, excluding only an exact authenticated worker lease.
pub fn holders_except(
    home: &Path,
    store: &str,
    except: Option<&Owner>,
) -> Result<Vec<u32>, SdkRefusal> {
    let mut live = Vec::new();
    for lease in leases(home, store)? {
        if lease.alive {
            if !except.is_some_and(|except| lease.owner.as_ref() == Some(except)) {
                live.push(lease.pid);
            }
            continue;
        }
        if !remove_file(&lease.path)? {
            continue;
        }
        home::log_line(
            home,
            "leases.jsonl",
            &SdkValue::map([
                ("at", SdkValue::Int(home::now_ms())),
                ("store", SdkValue::text(store)),
                (
                    "removed_stale_lease_of",
                    SdkValue::Int(i64::from(lease.pid)),
                ),
            ]),
        )?;
    }
    Ok(live)
}

/// Read only (the doctors, `store.get`, `store.list`): the live and the
/// stale holders of Store `store` other than this process; nothing is
/// removed and no lock is taken.
pub fn read_holders(home: &Path, store: &str) -> Result<(Vec<u32>, Vec<u32>), SdkRefusal> {
    let (mut live, mut stale) = (Vec::new(), Vec::new());
    for lease in leases(home, store)? {
        if lease.alive {
            live.push(lease.pid)
        } else {
            stale.push(lease.pid)
        }
    }
    Ok((live, stale))
}
