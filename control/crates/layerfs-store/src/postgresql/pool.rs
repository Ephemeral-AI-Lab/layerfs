//! A host worker's held PostgreSQL read sessions (#819 P-B): the sessions
//! the CLI's `file.read`, `file.stat` and `file.list` run on, kept open
//! between calls so a later call logs in nothing.
//!
//! Each session is a read service over one Full-checked connection
//! ([`crate::service::open_postgres_read`]), lent to one call at a time.
//! Sessions are kept per Store key (the record's binding,
//! [`crate::record::StoreRecord::postgresql_key`]) under one identity: the
//! record's non-secret login revision and the address dialled with the
//! login the call sent. Revisions have no order of their own; the Store's
//! record does. A call naming another revision than the sessions' is
//! compared, under the Store's lock, with the record's current revision:
//! the current one replaces the old identity (its idle sessions close at
//! once, its lent ones when they come back), and any other is refused
//! `Conflict` with nothing run and the sessions untouched. [`retire`] (a
//! `store connect` or `disconnect`) names the revision it retires and never
//! retires the record's current one.
//!
//! Before a held session serves a call it answers one readiness round trip
//! (`read_session_current`, a Light check). A call probes at most one held
//! session: when that probe fails, every idle session of the Store is closed
//! and the call opens exactly one new session. That is a route decision
//! taken before any byte of the call's operation is sent, as a mount
//! reader's `usable()` is, so nothing is retried. A call whose outcome is
//! `Unknown` or `StaleStore` drops the session it used.
//!
//! Bounds: [`PER_STORE`] sessions per Store and [`PER_PROCESS`] in all,
//! idle and lent. A call that finds no session free waits for one; it is
//! never refused `Busy`. Every lent session is given back by a guard, a
//! panicking call included, so a wait always progresses. An idle session of
//! another Store is closed to make room. The open-Store reaper closes
//! sessions unused for [`crate::open::IDLE`]; a drain closes them. A Store
//! with no session left forgets its identity and the dialled address, so
//! the password lives on only inside open sessions; the next call brings
//! the login again.
use crate::open_args::OpenArgs;
use crate::record::{self, StoreRecord};
use layerfs_bridge::control::SdkRefusal;
use layerfs_home::outcome::{refused, Outcome};
use layerfs_store_service::StoreService;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Condvar, Mutex, MutexGuard};
use std::time::{Duration, Instant};

/// Sessions one Store keeps, idle and lent (owner, 2026-11-27).
pub const PER_STORE: usize = 4;
/// Sessions one process keeps across its Stores (owner, 2026-11-27).
pub const PER_PROCESS: usize = 8;
/// How long a waiting call sleeps before it looks again; a given-back
/// session wakes it sooner.
const RECHECK: Duration = Duration::from_secs(1);

type Idle = Vec<(StoreService, Instant)>;

/// One Store key's sessions and identity. No `Debug`: `dial` holds the
/// password.
#[derive(Default)]
struct Pool {
    /// The login revision its sessions serve.
    revision: Option<String>,
    dial: String,
    /// Changed whenever the identity is replaced; a lent session of an older
    /// epoch is dropped when it comes back.
    epoch: u64,
    idle: Idle,
    lent: usize,
}

impl Pool {
    /// Replaces the identity: the idle sessions, to be dropped past the lock.
    fn replace(&mut self, epoch: u64, identity: Option<(&str, &str)>) -> Idle {
        self.revision = identity.map(|(revision, _)| revision.to_owned());
        self.dial = identity.map_or_else(String::new, |(_, dial)| dial.to_owned());
        self.epoch = epoch;
        std::mem::take(&mut self.idle)
    }
}

#[derive(Default)]
struct Pools {
    stores: BTreeMap<PathBuf, Pool>,
    epochs: u64,
    waiting: usize,
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

    /// Forgets Store `key` once it holds no session: its identity and the
    /// password-bearing address go with it.
    fn tidy(&mut self, key: &Path) {
        if (self.stores.get(key)).is_some_and(|pool| pool.lent == 0 && pool.idle.is_empty()) {
            self.stores.remove(key);
        }
    }

    /// The least recently used idle session of a Store other than `key`.
    fn evict(&mut self, key: &Path) -> Option<(StoreService, Instant)> {
        let (other, at) = (self.stores.iter())
            .filter(|(other, _)| other.as_path() != key)
            .filter_map(|(other, pool)| {
                let oldest = (0..pool.idle.len()).min_by_key(|&at| pool.idle[at].1)?;
                Some((other, oldest, pool.idle[oldest].1))
            })
            .min_by_key(|(_, _, last)| *last)
            .map(|(other, at, _)| (other.clone(), at))?;
        let evicted = self.stores.get_mut(&other)?.idle.swap_remove(at);
        self.tidy(&other);
        Some(evicted)
    }
}

static POOLS: Mutex<Pools> = Mutex::new(Pools {
    stores: BTreeMap::new(),
    epochs: 0,
    waiting: 0,
});
/// Signalled whenever a lent session comes back or a slot frees.
static FREED: Condvar = Condvar::new();

fn pools() -> MutexGuard<'static, Pools> {
    POOLS.lock().unwrap_or_else(|error| error.into_inner())
}

/// One lent session's slot. Dropping it gives the slot back on every path,
/// a panic included; the session is kept for later calls only when `keep`
/// was set and its identity still stands.
struct Lent {
    key: PathBuf,
    epoch: u64,
    service: Option<StoreService>,
    keep: bool,
}

impl Drop for Lent {
    fn drop(&mut self) {
        let mut all = pools();
        let mut dropped = self.service.take();
        if let Some(pool) = all.stores.get_mut(&self.key) {
            pool.lent -= 1;
            if self.keep && pool.epoch == self.epoch {
                if let Some(service) = dropped.take() {
                    pool.idle.push((service, Instant::now()));
                }
            }
        }
        all.tidy(&self.key);
        drop(all);
        drop(dropped);
        FREED.notify_all();
    }
}

/// Runs `call` once on a session of Store `store` of `home` (the key and
/// dialled address `open` names), admitted at login revision `revision`.
pub fn run(
    home: &Path,
    store: &str,
    revision: &str,
    open: &OpenArgs,
    call: impl FnOnce(&StoreService) -> Outcome,
) -> Outcome {
    let key = open.store_path()?;
    let dial = (open.postgresql.as_ref())
        .and_then(|postgresql| postgresql.url.as_deref())
        .ok_or_else(|| refused("InvalidInput", "a held read session needs its address"))?;
    let mut lent = checkout(home, store, &key, revision, dial, open)?;
    crate::open::reaper();
    layerfs_home::trace::step("store_open");
    let outcome = match &lent.service {
        Some(service) => call(service),
        None => Err(refused("Failed", "a held read session was not lent")),
    };
    layerfs_home::trace::step("call");
    lent.keep =
        !matches!(&outcome, Err(error) if matches!(error.code.as_str(), "Unknown" | "StaleStore"));
    drop(lent);
    outcome
}

fn checkout(
    home: &Path,
    store: &str,
    key: &Path,
    revision: &str,
    dial: &str,
    open: &OpenArgs,
) -> Result<Lent, SdkRefusal> {
    loop {
        let mut all = pools();
        let held = all.held();
        let next = all.next_epoch();
        let pool = all.stores.entry(key.to_path_buf()).or_default();
        if pool.revision.as_deref() != Some(revision) {
            all.tidy(key);
            drop(all);
            adopt(home, store, key, revision, dial)?;
            continue;
        }
        if pool.dial != dial {
            // the current revision with another login: the newer call's
            let dropped = pool.replace(next, Some((revision, dial)));
            drop(all);
            drop(dropped);
            FREED.notify_all();
            continue;
        }
        let epoch = pool.epoch;
        if let Some((service, _)) = pool.idle.pop() {
            pool.lent += 1;
            drop(all);
            let mut lent = Lent {
                key: key.to_path_buf(),
                epoch,
                service: Some(service),
                keep: false,
            };
            let probed = (lent.service.as_ref()).and_then(|s| s.opened.read_session_current());
            if probed == Some(true) {
                return Ok(lent);
            }
            // one probe per call: its Store's idle sessions go, and this
            // call opens one session in the slot it holds
            drop(lent.service.take());
            let stale = (pools().stores.get_mut(key)).map(|pool| std::mem::take(&mut pool.idle));
            drop(stale);
            lent.service = Some(crate::service::open_postgres_read(open)?);
            return Ok(lent);
        }
        if pool.lent < PER_STORE && held < PER_PROCESS {
            pool.lent += 1;
            drop(all);
            let mut lent = Lent {
                key: key.to_path_buf(),
                epoch,
                service: None,
                keep: false,
            };
            lent.service = Some(crate::service::open_postgres_read(open)?);
            return Ok(lent);
        }
        if held >= PER_PROCESS {
            if let Some(evicted) = all.evict(key) {
                drop(all);
                drop(evicted);
                continue;
            }
        }
        // every session this call may take is lent: wait for one, never Busy
        all.waiting += 1;
        let (mut all, _) = FREED
            .wait_timeout(all, RECHECK)
            .unwrap_or_else(|error| error.into_inner());
        all.waiting -= 1;
    }
}

/// Makes `revision` the identity of Store `key` if it is the record's
/// current revision, read under the Store's lock; any other is refused
/// `Conflict`, leaving the sessions as they are.
fn adopt(
    home: &Path,
    store: &str,
    key: &Path,
    revision: &str,
    dial: &str,
) -> Result<(), SdkRefusal> {
    let _lock = record::lock(home, store)?;
    let record = StoreRecord::of(home, store)?;
    if record.login_revision() != Some(revision) {
        return Err(refused(
            "Conflict",
            "the Store's login changed after this call was admitted; nothing was run",
        ));
    }
    let mut all = pools();
    let epoch = all.next_epoch();
    let pool = all.stores.entry(key.to_path_buf()).or_default();
    let dropped = match pool.revision.as_deref() == Some(revision) {
        true => Idle::new(),
        false => pool.replace(epoch, Some((revision, dial))),
    };
    drop(all);
    drop(dropped);
    FREED.notify_all();
    Ok(())
}

/// Closes every idle session unused for at least `idle`; how many.
pub fn close_idle(idle: Duration) -> usize {
    let mut all = pools();
    let mut closed = Vec::new();
    let keys: Vec<PathBuf> = all.stores.keys().cloned().collect();
    for key in keys {
        if let Some(pool) = all.stores.get_mut(&key) {
            let (old, kept): (Idle, Idle) = std::mem::take(&mut pool.idle)
                .into_iter()
                .partition(|(_, last)| last.elapsed() >= idle);
            pool.idle = kept;
            closed.extend(old);
        }
        all.tidy(&key);
    }
    drop(all);
    let count = closed.len();
    drop(closed);
    FREED.notify_all();
    count
}

/// Retires login revision `revision` of Store `store` of `home` (after a
/// `store connect` or `disconnect`), unless it is the record's current
/// revision: idle sessions serving it close now, lent ones when they come
/// back; how many closed now.
pub fn retire(home: &Path, store: &str, revision: &str) -> Result<usize, SdkRefusal> {
    // revisions are random and never reused: one read of the record (absent
    // after a disconnect) tells whether `revision` is the current one
    let current = StoreRecord::read(home, store)?;
    if current.as_ref().and_then(StoreRecord::login_revision) == Some(revision) {
        return Ok(0);
    }
    let folder = layerfs_home::home::paths::store_dir(home, store);
    let mut all = pools();
    let epoch = all.next_epoch();
    let keys: Vec<PathBuf> = (all.stores.iter())
        .filter(|(key, pool)| {
            key.parent() == Some(folder.as_path()) && pool.revision.as_deref() == Some(revision)
        })
        .map(|(key, _)| key.clone())
        .collect();
    let mut closed = Idle::new();
    for key in keys {
        if let Some(pool) = all.stores.get_mut(&key) {
            closed.extend(pool.replace(epoch, None));
        }
        all.tidy(&key);
    }
    drop(all);
    let count = closed.len();
    drop(closed);
    FREED.notify_all();
    Ok(count)
}

/// What the held sessions are now (§11, a host worker's ping).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Counts {
    /// Stores with a login and a session.
    pub stores: usize,
    pub idle: usize,
    pub lent: usize,
    /// Calls waiting for a session.
    pub waiting: usize,
}

/// The held sessions now.
pub fn counts() -> Counts {
    let all = pools();
    Counts {
        stores: all.stores.len(),
        idle: all.stores.values().map(|pool| pool.idle.len()).sum(),
        lent: all.stores.values().map(|pool| pool.lent).sum(),
        waiting: all.waiting,
    }
}
