//! A host worker's held PostgreSQL sessions (#819 P-B): the sessions the
//! CLI's `file.read`, `file.stat` and `file.list` and its file writes run
//! on, kept open between calls so a later call logs in nothing.
//!
//! A session is of one [`Kind`]. A read session is a read service over one
//! Full-checked connection ([`crate::service::open_postgres_read`]); a
//! write session is a writable Store service
//! ([`crate::service::open_postgres_write`]) with its writer, its lane
//! connection, its control reader and the Workspace readers its calls'
//! private Workspaces open, and its private overlay directory. Each is lent
//! to one call at a time. Sessions are kept per Store (its folder in the
//! home) under one identity: the record's non-secret login revision and the
//! address dialled with the login the call sent. Revisions have no order of
//! their own; the Store's record does. A call naming another revision than
//! the sessions' is compared, under the Store's lock, with the record's
//! current revision: the current one replaces the old identity (its idle
//! sessions close at once, its lent ones when they come back), and any
//! other is refused `Conflict` with nothing run and the sessions untouched.
//! [`retire`] (a `store connect`, `store upgrade` or `disconnect`) names
//! the revision it retires and never retires the record's current one.
//!
//! A call first holds a [`Reservation`]: one of its Store's session slots,
//! taken without waiting ([`reserve`], [`reserve_write`]). The host worker
//! starts a queued call only once its Store has a slot, so a call whose
//! Store is busy waits in its queue, on no thread, and a busy Store never
//! holds the threads another Store could use (§11). Nothing here waits for
//! a slot.
//!
//! Writes never starve reads: write slots lent at once are bounded by
//! [`WRITES_PER_STORE`] per Store and [`WRITES_PER_PROCESS`] in all, so at
//! least half of each bound always serves reads, and a write waiting on a
//! held Store writer keeps no read waiting for a slot.
//!
//! Before a held session serves a call it answers one readiness round trip
//! (`postgres_session_current`, a Light check, on the read session or the
//! write session's writer). A call probes at most one held session: when
//! that probe fails, every idle session of the Store is closed and the call
//! opens exactly one new session. That is a route decision taken before any
//! byte of the call's operation is sent, as a mount reader's `usable()` is,
//! so nothing is retried. A call whose outcome is `Unknown` or `StaleStore`
//! drops the session it used.
//!
//! Bounds: [`PER_STORE`] sessions per Store and [`PER_PROCESS`] in all, of
//! both kinds, idle, lent and reserved. An idle session of the other kind
//! or of another Store is closed to make room. Every slot is given back by
//! a guard, a panicking call included. The open-Store reaper closes
//! sessions unused for [`crate::admin::open::cache::IDLE`]; a drain closes them. A Store
//! with no session left forgets its identity, and the dialled address,
//! which holds the password, is overwritten before it is freed ([`wipe`]).
//! While a Store has sessions the address stays here, and each open session
//! keeps the login in its connection settings (a write session's reader
//! factory keeps one copy for the readers it opens).
use crate::admin::open::args::OpenArgs;
use crate::admin::record::store::{self, StoreRecord};
use layerfs_home::reply::outcome::{refused, Outcome};
use layerfs_home::secret::wipe::text as wipe;
use layerfs_protocol::sdk::SdkRefusal;
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
/// Write slots one Store lends at once: half of [`PER_STORE`].
pub const WRITES_PER_STORE: usize = PER_STORE / 2;
/// Write slots one process lends at once: half of [`PER_PROCESS`].
pub const WRITES_PER_PROCESS: usize = PER_PROCESS / 2;

/// What a session serves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// `fs.read`, `fs.stat` and `fs.list` on a read service.
    Read,
    /// File writes on a writable service.
    Write,
}

/// One held session. A write session closes its service and removes its
/// private overlay directory when dropped.
pub struct Session {
    service: Option<StoreService>,
    /// A write session's private overlay directory.
    overlay: Option<PathBuf>,
}

impl Session {
    fn kind(&self) -> Kind {
        match self.overlay {
            Some(_) => Kind::Write,
            None => Kind::Read,
        }
    }

    fn open(kind: Kind, open: &OpenArgs) -> Result<Self, SdkRefusal> {
        Ok(match kind {
            Kind::Read => Self {
                service: Some(crate::admin::open::service::open_postgres_read(open)?),
                overlay: None,
            },
            Kind::Write => {
                let (service, overlay) = crate::admin::open::service::open_postgres_write(open)?;
                Self {
                    service: Some(service),
                    overlay: Some(overlay),
                }
            }
        })
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        if let Some(service) = self.service.take() {
            // a write session's overlay owner stops; one attempt, and its
            // failure leaves nothing a later call meets
            let _ = service.close();
        }
        if let Some(overlay) = self.overlay.take() {
            let _ = std::fs::remove_dir_all(overlay);
        }
    }
}

type Idle = Vec<(Session, Instant)>;

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
    /// Of `lent`, the write slots.
    writes: usize,
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

    fn writes(&self) -> usize {
        self.stores.values().map(|pool| pool.writes).sum()
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

    /// The least recently used idle session of Store `key` alone (`own`),
    /// or of any Store; `key`'s own idle sessions are all of the other kind
    /// here, since one of the call's kind would have been taken.
    fn evict(&mut self, key: &Path, own: bool) -> Option<Session> {
        let (other, at) = (self.stores.iter())
            .filter(|(other, _)| !own || other.as_path() == key)
            .filter_map(|(other, pool)| {
                let oldest = (0..pool.idle.len()).min_by_key(|&at| pool.idle[at].1)?;
                Some((other, oldest, pool.idle[oldest].1))
            })
            .min_by_key(|(_, _, last)| *last)
            .map(|(other, at, _)| (other.clone(), at))?;
        let (evicted, _) = self.stores.get_mut(&other)?.idle.swap_remove(at);
        // `key` keeps its identity: the caller lends it a slot now
        if other != key {
            self.tidy(&other);
        }
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
    kind: Kind,
    /// The idle session this slot took, with the epoch it was kept in.
    session: Option<(Session, u64)>,
    /// An idle session of the other kind or of another Store, closed to
    /// make room, past the locks.
    evicted: Option<Session>,
    armed: bool,
}

impl Reservation {
    /// What the slot serves.
    pub fn kind(&self) -> Kind {
        self.kind
    }
}

impl Drop for Reservation {
    fn drop(&mut self) {
        drop(self.evicted.take());
        let mut unused = self.session.take();
        if self.armed {
            let mut all = pools();
            if let Some(pool) = all.stores.get_mut(&self.key) {
                pool.lent -= 1;
                pool.writes -= usize::from(self.kind == Kind::Write);
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

/// A read session slot of Store `store` of `home`, if one is free now: an
/// idle read session, taken now, room under both bounds, or room made by
/// closing the least recently used idle session of the other kind or of
/// another Store. Never waits.
///
/// The slot is taken before the call's record is checked (a file read
/// under the Store's lock, not done while the worker's queue is locked): a
/// call then refused there may have closed another idle session, which its
/// Store's next call replaces with one login.
pub fn reserve(home: &Path, store: &str) -> Option<Reservation> {
    reserve_kind(home, store, Kind::Read)
}

/// [`reserve`] for a write: also refused (`None`) while the Store lends
/// [`WRITES_PER_STORE`] write slots or the process [`WRITES_PER_PROCESS`].
pub fn reserve_write(home: &Path, store: &str) -> Option<Reservation> {
    reserve_kind(home, store, Kind::Write)
}

/// A slot of `kind` of Store `store` of `home`, if one is free now.
pub fn reserve_kind(home: &Path, store: &str, kind: Kind) -> Option<Reservation> {
    let key = layerfs_home::disk::paths::store_dir(home, store);
    let mut all = pools();
    let (held, writes) = (all.held(), all.writes());
    let pool = all.stores.entry(key.clone()).or_default();
    let write = kind == Kind::Write;
    if write && (pool.writes >= WRITES_PER_STORE || writes >= WRITES_PER_PROCESS) {
        all.tidy(&key);
        return None;
    }
    let slot = |pool: &mut Pool, session, evicted| {
        pool.lent += 1;
        pool.writes += usize::from(write);
        Some(Reservation {
            key: key.clone(),
            kind,
            session,
            evicted,
            armed: true,
        })
    };
    let same = (pool.idle.iter()).rposition(|(session, _)| session.kind() == kind);
    if let Some(at) = same {
        let (session, _) = pool.idle.remove(at);
        let epoch = pool.epoch;
        return slot(pool, Some((session, epoch)), None);
    }
    let stored = pool.lent + pool.idle.len();
    if stored < PER_STORE && held < PER_PROCESS {
        return slot(pool, None, None);
    }
    // the Store at its bound frees one of its own idle sessions (of the
    // other kind); one under it, any Store's least recently used
    let own = stored >= PER_STORE;
    let evicted = (pool.lent < PER_STORE)
        .then(|| all.evict(&key, own))
        .flatten();
    match (evicted, all.stores.get_mut(&key)) {
        (Some(evicted), Some(pool)) => slot(pool, None, Some(evicted)),
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
    service: Option<Session>,
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
            pool.writes -= usize::from(self.reservation.kind == Kind::Write);
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

/// Runs read `call` once on a read session of Store `store` of `home` (the
/// address `open` names), admitted at login revision `revision`, in the
/// slot this thread holds ([`hold`]) or one free now; with none, the call
/// is refused `Unavailable` with nothing run.
pub fn run(
    home: &Path,
    store: &str,
    revision: &str,
    open: &OpenArgs,
    call: impl FnOnce(&StoreService) -> Outcome,
) -> Outcome {
    run_kind(home, store, revision, (Kind::Read, open), call)
}

/// [`run`] for a write, on a write session.
pub fn run_write(
    home: &Path,
    store: &str,
    revision: &str,
    open: &OpenArgs,
    call: impl FnOnce(&StoreService) -> Outcome,
) -> Outcome {
    run_kind(home, store, revision, (Kind::Write, open), call)
}

fn run_kind(
    home: &Path,
    store: &str,
    revision: &str,
    (kind, open): (Kind, &OpenArgs),
    call: impl FnOnce(&StoreService) -> Outcome,
) -> Outcome {
    let key = layerfs_home::disk::paths::store_dir(home, store);
    let dial = (open.postgresql.as_ref())
        .and_then(|postgresql| postgresql.url.as_deref())
        .ok_or_else(|| refused("InvalidInput", "a held session needs its address"))?;
    let held = HELD.with(|held| held.borrow_mut().take());
    let reservation = match held {
        Some(reservation) if reservation.key == key && reservation.kind == kind => {
            Some(reservation)
        }
        other => {
            drop(other);
            reserve_kind(home, store, kind)
        }
    }
    .ok_or_else(|| {
        refused(
            "Unavailable",
            format!("every PostgreSQL session of Store {store} is in use; nothing was run"),
        )
    })?;
    let mut lent = checkout(home, store, reservation, revision, dial, open)?;
    crate::admin::open::close::reaper();
    layerfs_home::log::trace::step("store_open");
    let outcome = match lent.service.as_ref().and_then(|held| held.service.as_ref()) {
        Some(service) => call(service),
        None => Err(refused("Failed", "a held session was not lent")),
    };
    layerfs_home::log::trace::step("call");
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
    let (key, kind) = (reservation.key.clone(), reservation.kind);
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
            None => (pool.idle.iter())
                .rposition(|(session, _)| session.kind() == kind)
                .map(|at| (pool.idle.remove(at).0, pool.epoch)),
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
    let probed = (lent.service.as_ref())
        .and_then(|held| held.service.as_ref())
        .map(|service| service.opened.postgres_session_current());
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
    // a refused new session (a server restart, say) is said plainly
    let opened = Session::open(kind, open)
        .map_err(|refusal| crate::backend::postgresql::open::new_session_refused(store, refusal))?;
    lent.service = Some(opened);
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
    let _lock = store::lock(home, store)?;
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
    let key = layerfs_home::disk::paths::store_dir(home, store);
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
    /// Of `lent`, the write slots.
    pub writes: usize,
}

/// The held sessions now.
pub fn counts() -> Counts {
    let all = pools();
    Counts {
        stores: all.stores.len(),
        idle: all.stores.values().map(|pool| pool.idle.len()).sum(),
        lent: all.stores.values().map(|pool| pool.lent).sum(),
        writes: all.writes(),
    }
}
