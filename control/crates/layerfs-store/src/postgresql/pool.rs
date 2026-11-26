//! A host worker's held PostgreSQL read sessions (#819 P-B): the sessions
//! the CLI's `file.read`, `file.stat` and `file.list` run on, kept open
//! between calls so a later call logs in nothing.
//!
//! Each session is a read service over one Full-checked connection
//! ([`crate::service::open_postgres_read`]), lent to one call at a time.
//! Sessions are kept per Store (its folder in the home) under one identity:
//! the record's non-secret login revision and the address dialled with the
//! login the call sent. Revisions have no order of their own; the Store's
//! record does. A call naming another revision than the sessions' is
//! compared, under the Store's lock, with the record's current revision:
//! the current one replaces the old identity (its idle sessions close at
//! once, its lent ones when they come back), and any other is refused
//! `Conflict` with nothing run and the sessions untouched. [`retire`] (a
//! `store connect`, `store upgrade` or `disconnect`) names the revision it
//! retires and never retires the record's current one.
//!
//! A call first holds a [`Reservation`]: one of its Store's session slots,
//! taken without waiting ([`reserve`]). The host worker's PostgreSQL lane
//! starts a queued read only once its Store has a slot, so a read whose
//! Store is busy waits in the queue, on no thread, and a busy Store never
//! holds the threads another Store could use (§11). Nothing here waits for a
//! slot.
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
//! idle, lent and reserved. An idle session of another Store is closed to
//! make room. Every slot is given back by a guard, a panicking call
//! included. The open-Store reaper closes sessions unused for
//! [`crate::open::IDLE`]; a drain closes them. A Store with no session left
//! forgets its identity, and the dialled address, which holds the password,
//! is overwritten before it is freed ([`wipe`]). While a Store has sessions
//! the address stays here, and each open session keeps the login in its
//! connection settings.
use crate::open_args::OpenArgs;
use crate::record::{self, StoreRecord};
use layerfs_bridge::control::SdkRefusal;
use layerfs_home::outcome::{refused, Outcome};
use layerfs_home::wipe::text as wipe;
use layerfs_store_service::StoreService;
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

/// Sessions one Store keeps, idle, lent and reserved (owner, 2026-11-27).
pub const PER_STORE: usize = 4;
/// Sessions one process keeps across its Stores (owner, 2026-11-27).
pub const PER_PROCESS: usize = 8;

type Idle = Vec<(StoreService, Instant)>;

/// One Store's sessions and identity. No `Debug`: `dial` holds the
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
    /// Slots reserved or lent.
    lent: usize,
}

impl Pool {
    /// Replaces the identity: the idle sessions, to be dropped past the lock.
    fn replace(&mut self, epoch: u64, identity: Option<(&str, &str)>) -> Idle {
        self.revision = identity.map(|(revision, _)| revision.to_owned());
        let dial = identity.map_or_else(String::new, |(_, dial)| dial.to_owned());
        wipe(std::mem::replace(&mut self.dial, dial));
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

    /// Forgets Store `key` once it holds no session: its identity goes and
    /// its password-bearing address is overwritten.
    fn tidy(&mut self, key: &Path) {
        if (self.stores.get(key)).is_some_and(|pool| pool.lent == 0 && pool.idle.is_empty()) {
            if let Some(pool) = self.stores.remove(key) {
                wipe(pool.dial);
            }
        }
    }

    /// The least recently used idle session of a Store other than `key`.
    fn evict(&mut self, key: &Path) -> Option<StoreService> {
        let (other, at) = (self.stores.iter())
            .filter(|(other, _)| other.as_path() != key)
            .filter_map(|(other, pool)| {
                let oldest = (0..pool.idle.len()).min_by_key(|&at| pool.idle[at].1)?;
                Some((other, oldest, pool.idle[oldest].1))
            })
            .min_by_key(|(_, _, last)| *last)
            .map(|(other, at, _)| (other.clone(), at))?;
        let (evicted, _) = self.stores.get_mut(&other)?.idle.swap_remove(at);
        self.tidy(&other);
        Some(evicted)
    }
}

static POOLS: Mutex<Pools> = Mutex::new(Pools {
    stores: BTreeMap::new(),
    epochs: 0,
});

fn pools() -> MutexGuard<'static, Pools> {
    POOLS.lock().unwrap_or_else(|error| error.into_inner())
}

/// One of a Store's session slots, held before a call starts. Dropped
/// unused, it gives the slot back, and the idle session it took to the
/// Store's idle ones while their identity stands.
pub struct Reservation {
    key: PathBuf,
    /// The idle session this slot took, with the epoch it was kept in.
    session: Option<(StoreService, u64)>,
    /// An idle session of another Store, closed to make room, past the
    /// locks.
    evicted: Option<StoreService>,
    armed: bool,
}

impl Drop for Reservation {
    fn drop(&mut self) {
        drop(self.evicted.take());
        let mut unused = self.session.take();
        if self.armed {
            let mut all = pools();
            if let Some(pool) = all.stores.get_mut(&self.key) {
                pool.lent -= 1;
                if let Some((service, epoch)) = unused.take() {
                    match epoch == pool.epoch {
                        true => pool.idle.push((service, Instant::now())),
                        false => unused = Some((service, epoch)),
                    }
                }
            }
            all.tidy(&self.key);
        }
        drop(unused);
    }
}

/// A session slot of Store `store` of `home`, if one is free now: an idle
/// session, taken now, room under both bounds, or room made by closing
/// another Store's least recently used idle session. Never waits.
///
/// The slot is taken before the read's record is checked (a file read
/// under the Store's lock, not done while the worker's queue is locked): a
/// read then refused there may have closed another Store's idle session,
/// which that Store's next read replaces with one login.
pub fn reserve(home: &Path, store: &str) -> Option<Reservation> {
    let key = layerfs_home::home::paths::store_dir(home, store);
    let mut all = pools();
    let held = all.held();
    let pool = all.stores.entry(key.clone()).or_default();
    if let Some((service, _)) = pool.idle.pop() {
        pool.lent += 1;
        return Some(Reservation {
            key,
            session: Some((service, pool.epoch)),
            evicted: None,
            armed: true,
        });
    }
    let room = pool.lent < PER_STORE;
    if room && held < PER_PROCESS {
        pool.lent += 1;
        return Some(Reservation {
            key,
            session: None,
            evicted: None,
            armed: true,
        });
    }
    let evicted = room.then(|| all.evict(&key)).flatten();
    match (evicted, all.stores.get_mut(&key)) {
        (Some(evicted), Some(pool)) => {
            pool.lent += 1;
            Some(Reservation {
                key,
                session: None,
                evicted: Some(evicted),
                armed: true,
            })
        }
        (evicted, _) => {
            all.tidy(&key);
            drop(all);
            drop(evicted);
            None
        }
    }
}

thread_local! {
    /// The reservation the host worker's PostgreSQL lane took for the read
    /// this thread runs now.
    static HELD: RefCell<Option<Reservation>> = const { RefCell::new(None) };
}

/// Hands `reservation` to the read this thread runs next.
pub fn hold(reservation: Reservation) {
    let replaced = HELD.with(|held| held.borrow_mut().replace(reservation));
    drop(replaced);
}

/// Gives back a reservation this thread's read did not use.
pub fn release_held() {
    let held = HELD.with(|held| held.borrow_mut().take());
    drop(held);
}

/// One lent session's slot. Dropping it gives the slot back on every path,
/// a panic included; the session is kept for later calls only when `keep`
/// was set and its identity still stands.
struct Lent {
    reservation: Reservation,
    epoch: u64,
    service: Option<StoreService>,
    keep: bool,
}

impl Drop for Lent {
    fn drop(&mut self) {
        let key = self.reservation.key.clone();
        self.reservation.armed = false;
        let mut all = pools();
        let mut dropped = self.service.take();
        if let Some(pool) = all.stores.get_mut(&key) {
            pool.lent -= 1;
            if self.keep && pool.epoch == self.epoch {
                if let Some(service) = dropped.take() {
                    pool.idle.push((service, Instant::now()));
                }
            }
        }
        all.tidy(&key);
        drop(all);
        drop(dropped);
    }
}

/// Runs `call` once on a session of Store `store` of `home` (the address
/// `open` names), admitted at login revision `revision`, in the slot this
/// thread holds ([`hold`]) or one free now; with none, the call is refused
/// `Unavailable` with nothing run.
pub fn run(
    home: &Path,
    store: &str,
    revision: &str,
    open: &OpenArgs,
    call: impl FnOnce(&StoreService) -> Outcome,
) -> Outcome {
    let key = layerfs_home::home::paths::store_dir(home, store);
    let dial = (open.postgresql.as_ref())
        .and_then(|postgresql| postgresql.url.as_deref())
        .ok_or_else(|| refused("InvalidInput", "a held read session needs its address"))?;
    let held = HELD.with(|held| held.borrow_mut().take());
    let reservation = match held {
        Some(reservation) if reservation.key == key => Some(reservation),
        _ => reserve(home, store),
    }
    .ok_or_else(|| {
        refused(
            "Unavailable",
            format!("every PostgreSQL session of Store {store} is in use; nothing was run"),
        )
    })?;
    let mut lent = checkout(home, store, reservation, revision, dial, open)?;
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
    mut reservation: Reservation,
    revision: &str,
    dial: &str,
    open: &OpenArgs,
) -> Result<Lent, SdkRefusal> {
    drop(reservation.evicted.take());
    let key = reservation.key.clone();
    let mut taken = reservation.session.take();
    let (epoch, idle) = loop {
        let mut all = pools();
        let next = all.next_epoch();
        let Some(pool) = all.stores.get_mut(&key) else {
            return Err(refused("Failed", "a reserved session slot was lost"));
        };
        if pool.revision.as_deref() != Some(revision) {
            drop(all);
            if let Err(refusal) = adopt(home, store, &key, revision, dial) {
                reservation.session = taken;
                return Err(refusal);
            }
            continue;
        }
        if pool.dial != dial {
            // the current revision with another login: the newer call's
            let dropped = pool.replace(next, Some((revision, dial)));
            drop(all);
            drop(dropped);
            continue;
        }
        // the session the slot took, unless its identity was replaced since
        // (then closed past the lock)
        let stale = taken.take_if(|(_, epoch)| *epoch != pool.epoch);
        let idle = match taken.take() {
            Some(kept) => Some(kept),
            None => pool.idle.pop().map(|(service, _)| (service, pool.epoch)),
        };
        let epoch = pool.epoch;
        drop(all);
        drop(stale);
        break (epoch, idle);
    };
    let mut lent = Lent {
        reservation,
        epoch,
        service: idle.map(|(service, _)| service),
        keep: false,
    };
    let probed = (lent.service.as_ref()).map(|service| service.opened.read_session_current());
    match probed {
        Some(Some(true)) => return Ok(lent),
        Some(_) => {
            // one probe per call: its Store's idle sessions go, and this
            // call opens one session in the slot it holds
            drop(lent.service.take());
            let stale = (pools().stores.get_mut(&key)).map(|pool| std::mem::take(&mut pool.idle));
            drop(stale);
        }
        None => {}
    }
    lent.service = Some(crate::service::open_postgres_read(open)?);
    Ok(lent)
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
    count
}

/// Retires login revision `revision` of Store `store` of `home` (after a
/// `store connect`, `store upgrade` or `disconnect`), unless it is the record's current
/// revision: idle sessions serving it close now, lent ones when they come
/// back; how many closed now.
pub fn retire(home: &Path, store: &str, revision: &str) -> Result<usize, SdkRefusal> {
    // revisions are random and never reused: one read of the record (absent
    // after a disconnect) tells whether `revision` is the current one
    let current = StoreRecord::read(home, store)?;
    if current.as_ref().and_then(StoreRecord::login_revision) == Some(revision) {
        return Ok(0);
    }
    let key = layerfs_home::home::paths::store_dir(home, store);
    let mut all = pools();
    let epoch = all.next_epoch();
    let closed = match all.stores.get_mut(&key) {
        Some(pool) if pool.revision.as_deref() == Some(revision) => pool.replace(epoch, None),
        _ => Idle::new(),
    };
    all.tidy(&key);
    drop(all);
    let count = closed.len();
    drop(closed);
    Ok(count)
}

/// What the held sessions are now (§11, a host worker's ping).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Counts {
    /// Stores with a session or a slot taken.
    pub stores: usize,
    pub idle: usize,
    /// Slots reserved or lent.
    pub lent: usize,
}

/// The held sessions now.
pub fn counts() -> Counts {
    let all = pools();
    Counts {
        stores: all.stores.len(),
        idle: all.stores.values().map(|pool| pool.idle.len()).sum(),
        lent: all.stores.values().map(|pool| pool.lent).sum(),
    }
}
