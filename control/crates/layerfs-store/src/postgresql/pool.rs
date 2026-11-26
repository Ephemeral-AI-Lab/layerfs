//! A host worker's held PostgreSQL read sessions (#819 P-B): the sessions
//! the CLI's `file.read`, `file.stat` and `file.list` run on, kept open
//! between calls so a later call logs in nothing.
//!
//! Each session is a read service over one Full-checked connection
//! ([`crate::service::open_postgres_read`]), lent to one call at a time.
//! Sessions are kept per Store key (the record's binding,
//! [`crate::record::StoreRecord::postgresql_key`]) under one identity: the
//! record's non-secret login revision and the address dialled with the
//! login the call sent. A call at a newer revision retires the old one:
//! its idle sessions close at once, its lent ones when they come back, and
//! a call still naming it (admitted before the change) is refused
//! `Conflict` with nothing run, so no session is opened at it again.
//! [`retire_store`] (a `store connect`, `disconnect` or upgrade) retires
//! the current one the same way.
//!
//! Before a held session serves a call it answers one readiness round trip
//! (`read_session_current`): a lost connection, a late answer or another
//! catalog generation (a compaction swap, an upgrade) drops it and the
//! call takes another session or opens a new one. Nothing of the call has
//! been sent then, so nothing is retried. A call whose outcome is
//! `Unknown` or `StaleStore` drops the session it used.
//!
//! Bounds: [`PER_STORE`] sessions per Store and [`PER_PROCESS`] in all,
//! idle and lent, and the last [`RETIRED`] revisions per Store. A call that
//! finds no session free waits for one; it is never refused `Busy`. An idle
//! session of another Store is closed to make room. The open-Store reaper
//! closes sessions unused for [`crate::open::IDLE`]; a drain closes them.
use crate::open_args::OpenArgs;
use layerfs_bridge::control::SdkRefusal;
use layerfs_home::outcome::{refused, Outcome};
use layerfs_store_service::StoreService;
use std::collections::{BTreeMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Condvar, Mutex, MutexGuard};
use std::time::{Duration, Instant};

/// Sessions one Store keeps, idle and lent (provisional, §11).
pub const PER_STORE: usize = 4;
/// Sessions one process keeps across its Stores (provisional, §11).
pub const PER_PROCESS: usize = 8;
/// Retired login revisions remembered per Store.
pub const RETIRED: usize = 16;

type Idle = Vec<(StoreService, Instant)>;

/// One Store key's sessions and identities. No `Debug`: `dial` holds the
/// password.
#[derive(Default)]
struct Pool {
    /// The identity its sessions serve, once a call named one.
    revision: Option<String>,
    dial: String,
    /// Changed whenever an identity is retired; a lent session of an older
    /// epoch is dropped when it comes back.
    epoch: u64,
    idle: Idle,
    lent: usize,
    retired: VecDeque<String>,
}

impl Pool {
    /// Retires the current identity: the idle sessions, to be dropped past
    /// the lock.
    fn retire(&mut self, epoch: u64) -> Idle {
        if let Some(revision) = self.revision.take() {
            if self.retired.len() == RETIRED {
                self.retired.pop_front();
            }
            self.retired.push_back(revision);
        }
        self.dial.clear();
        self.epoch = epoch;
        std::mem::take(&mut self.idle)
    }
}

#[derive(Default)]
struct Pools {
    stores: BTreeMap<PathBuf, Pool>,
    epochs: u64,
}

impl Pools {
    fn held(&self) -> usize {
        let held = self.stores.values().map(|pool| pool.lent + pool.idle.len());
        held.sum()
    }

    fn next_epoch(&mut self) -> u64 {
        self.epochs += 1;
        self.epochs
    }

    /// The least recently used idle session of a Store other than `key`.
    fn evict(&mut self, key: &Path) -> Option<(StoreService, Instant)> {
        let (pool, at) = (self.stores.iter_mut())
            .filter(|(other, _)| other.as_path() != key)
            .filter_map(|(_, pool)| {
                let oldest = (0..pool.idle.len()).min_by_key(|&at| pool.idle[at].1)?;
                Some((pool, oldest))
            })
            .min_by_key(|(pool, at)| pool.idle[*at].1)?;
        Some(pool.idle.swap_remove(at))
    }
}

static POOLS: Mutex<Pools> = Mutex::new(Pools {
    stores: BTreeMap::new(),
    epochs: 0,
});
/// Signalled whenever a lent session comes back or a slot frees.
static FREED: Condvar = Condvar::new();

fn pools() -> MutexGuard<'static, Pools> {
    POOLS.lock().unwrap_or_else(|error| error.into_inner())
}

/// Runs `call` once on a session of the Store `open` names (its key and its
/// dialled address) at login revision `revision`.
pub(crate) fn run(
    revision: &str,
    open: &OpenArgs,
    call: impl FnOnce(&StoreService) -> Outcome,
) -> Outcome {
    let key = open.store_path()?;
    let dial = (open.postgresql.as_ref())
        .and_then(|postgresql| postgresql.url.as_deref())
        .ok_or_else(|| refused("InvalidInput", "a held read session needs its address"))?;
    let (service, epoch) = checkout(&key, revision, dial, open)?;
    layerfs_home::trace::step("store_open");
    let outcome = call(&service);
    layerfs_home::trace::step("call");
    let lost =
        matches!(&outcome, Err(error) if matches!(error.code.as_str(), "Unknown" | "StaleStore"));
    checkin(&key, epoch, service, !lost);
    outcome
}

fn checkout(
    key: &Path,
    revision: &str,
    dial: &str,
    open: &OpenArgs,
) -> Result<(StoreService, u64), SdkRefusal> {
    loop {
        let mut all = pools();
        let held = all.held();
        let epoch = all.next_epoch();
        let pool = all.stores.entry(key.to_path_buf()).or_default();
        if pool.retired.iter().any(|retired| retired == revision) {
            return Err(refused(
                "Conflict",
                "the Store's login changed after this call was admitted; nothing was run",
            ));
        }
        if pool.revision.as_deref() != Some(revision) || pool.dial != dial {
            // a newer login: no later call uses the old one
            let retired = match pool.revision.as_deref() {
                Some(current) if current != revision => pool.retire(epoch),
                _ => {
                    pool.epoch = epoch;
                    std::mem::take(&mut pool.idle)
                }
            };
            pool.revision = Some(revision.to_owned());
            pool.dial = dial.to_owned();
            drop(all);
            drop(retired);
            FREED.notify_all();
            continue;
        }
        if let Some((service, _)) = pool.idle.pop() {
            pool.lent += 1;
            let epoch = pool.epoch;
            drop(all);
            if service.opened.read_session_current() == Some(true) {
                return Ok((service, epoch));
            }
            drop(service);
            give_back(key);
            continue;
        }
        if pool.lent < PER_STORE && held < PER_PROCESS {
            pool.lent += 1;
            let epoch = pool.epoch;
            drop(all);
            return match crate::service::open_postgres_read(open) {
                Ok(service) => Ok((service, epoch)),
                Err(refusal) => {
                    give_back(key);
                    Err(refusal)
                }
            };
        }
        if held >= PER_PROCESS {
            if let Some(evicted) = all.evict(key) {
                drop(all);
                drop(evicted);
                continue;
            }
        }
        // every session this call may take is lent: wait for one, never Busy
        drop(FREED.wait(all).unwrap_or_else(|error| error.into_inner()));
    }
}

/// A lent session's slot is free again, its session dropped.
fn give_back(key: &Path) {
    if let Some(pool) = pools().stores.get_mut(key) {
        pool.lent -= 1;
    }
    FREED.notify_all();
}

fn checkin(key: &Path, epoch: u64, service: StoreService, kept: bool) {
    let mut all = pools();
    let dropped = match all.stores.get_mut(key) {
        Some(pool) => {
            pool.lent -= 1;
            if kept && pool.epoch == epoch {
                pool.idle.push((service, Instant::now()));
                None
            } else {
                Some(service)
            }
        }
        None => Some(service),
    };
    drop(all);
    drop(dropped);
    FREED.notify_all();
    crate::open::reaper();
}

/// Closes every idle session unused for at least `idle`; how many.
pub fn close_idle(idle: Duration) -> usize {
    let mut all = pools();
    let mut closed = Vec::new();
    for pool in all.stores.values_mut() {
        let (old, kept): (Idle, Idle) = std::mem::take(&mut pool.idle)
            .into_iter()
            .partition(|(_, last)| last.elapsed() >= idle);
        pool.idle = kept;
        closed.extend(old);
    }
    drop(all);
    let count = closed.len();
    drop(closed);
    FREED.notify_all();
    count
}

/// Retires the current login of Store `store` of `home` (a `store
/// connect`, `disconnect` or upgrade): its idle sessions close now, its
/// lent ones when they come back; how many closed now.
pub fn retire_store(home: &Path, store: &str) -> usize {
    let folder = layerfs_home::home::paths::store_dir(home, store);
    let mut all = pools();
    let epoch = all.next_epoch();
    let closed: Idle = (all.stores.iter_mut())
        .filter(|(key, _)| key.parent() == Some(folder.as_path()))
        .flat_map(|(_, pool)| pool.retire(epoch))
        .collect();
    drop(all);
    let count = closed.len();
    drop(closed);
    FREED.notify_all();
    count
}

/// Sessions held now: `(idle, lent)`.
pub fn counts() -> (usize, usize) {
    let all = pools();
    let idle = all.stores.values().map(|pool| pool.idle.len()).sum();
    let lent = all.stores.values().map(|pool| pool.lent).sum();
    (idle, lent)
}
