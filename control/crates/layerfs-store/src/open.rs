//! The Stores this process holds open: a call opens its Store once and later
//! calls reuse it; a Store unused for [`IDLE`] is closed by a reaper thread
//! (H §7.7). The same cache serves the caller's in-process route and a
//! machine's worker. Each entry keeps what must live as long as the open
//! Store: its lease. A Store with a call running is never closed: the
//! reaper skips it and `close` refuses `InUse`.
//!
//! When the last call on a SQLite Store returns, or a Sandbox on it closes,
//! with no other call running here, the Store's write-ahead log is
//! checkpointed and truncated once (#86), so a quiesced Store's size is its
//! file. The Store stays open. After a call this runs on its own thread once
//! the call is answered; after `sandbox.remove` it runs before that answer, so
//! a `store get --with live` right after sees the truncated log. A checkpoint that
//! cannot complete now (another process writing, a reader still in the log)
//! is left for the idle close and never retried.
use crate::lease::{self, Lease};
use crate::open_args::OpenArgs;
use crate::record::{self, StoreRecord, StoreRef};
use crate::service::{self as direct, Completion, Owner, Slot};
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_home::home::paths;
use layerfs_home::outcome::{failed, refused, with_next, Outcome};
use std::cell::Cell;
use std::collections::BTreeMap;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, Once};
use std::time::{Duration, Instant};

/// How long an unused Store stays open.
pub const IDLE: Duration = Duration::from_secs(60);

const HOST_STORES: usize = 16;

struct Entry {
    /// What it was opened with: what `direct::release` needs.
    args: OpenArgs,
    last: Instant,
    /// Calls running on it now.
    calls: usize,
    /// Idle checkpoints running on it now (#86); it stays open meanwhile.
    settling: usize,
    checkpoint: Result<Option<bool>, SdkRefusal>,
    lease: Option<Lease>,
}

struct Cached {
    host: Option<PathBuf>,
    state: Slot<Entry, CloseReport>,
}

type Cache = BTreeMap<PathBuf, Cached>;
static OPEN: Mutex<Cache> = Mutex::new(BTreeMap::new());
static REAPER: Once = Once::new();
/// Idle checkpoints and capacity waiters share these state notifications.
static CHANGED: Condvar = Condvar::new();

fn cache() -> MutexGuard<'static, Cache> {
    OPEN.lock().unwrap_or_else(|e| e.into_inner())
}

fn wait_changed(open: MutexGuard<'static, Cache>) {
    drop(CHANGED.wait(open).unwrap_or_else(|e| e.into_inner()));
}

impl Cached {
    fn ready(&mut self) -> Option<&mut Entry> {
        match &mut self.state {
            Slot::Ready(entry) => Some(entry),
            _ => None,
        }
    }
}

impl Entry {
    fn admit(&mut self, calls: usize) {
        self.last = Instant::now();
        self.calls += calls;
    }
}

thread_local! {
    static ONE_SHOT: Cell<bool> = const { Cell::new(false) };
}

/// Gives a single-call process's Store calls the one-shot route.
pub fn with_one_shot<T>(enabled: bool, call: impl FnOnce() -> T) -> T {
    struct Restore(bool);
    impl Drop for Restore {
        fn drop(&mut self) {
            ONE_SHOT.set(self.0);
        }
    }
    let _restore = Restore(ONE_SHOT.replace(enabled));
    call()
}

/// Runs Store method `op` once, with the call's own arguments `args`, on
/// the Store `open` names (`storePath`), opening it first if this process
/// holds it not yet. `lease` is held for as long as the Store stays open.
/// The Store stays open while it runs.
pub fn run(op: &str, open: &OpenArgs, args: SdkValue, lease: Option<Lease>) -> Outcome {
    if ONE_SHOT.get() && open.postgresql.is_some() && direct::is_one_shot_read(op) {
        let _lease = lease;
        return direct::postgres_read(op, open, args);
    }
    let running = Running {
        path: enter(open, lease, 1)?,
        owner: Owner::Direct,
    };
    call(op, open, args, running)
}

/// Runs one host worker method from the real home. The worker takes its own
/// lease and validates the Store under the same lock as maintenance on
/// every admission, including a hit. Routing and the method allowlist are
/// checked by the caller before it invokes this.
pub fn run_host(
    home: &Path,
    store: &str,
    project: &str,
    method: &str,
    fields: SdkValue,
) -> Outcome {
    let SdkValue::Map(mut fields) = fields else {
        return Err(refused(
            "InvalidInput",
            "a Store call needs an argument map",
        ));
    };
    let (open, running) = enter_host(home, store, project)?;
    fields.retain(|(name, _)| name != "project");
    fields.push(("project".to_owned(), SdkValue::text(project)));
    call(method, &open, SdkValue::Map(fields), running)
}

/// [`run_host`] for a write whose content or changes `source` streams.
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub fn run_host_streamed(
    home: &Path,
    store: &str,
    project: &str,
    method: &str,
    fields: SdkValue,
    source: &mut dyn layerfs_store_service::methods::Source,
) -> Outcome {
    let SdkValue::Map(mut fields) = fields else {
        return Err(refused(
            "InvalidInput",
            "a Store call needs an argument map",
        ));
    };
    let (open, running) = enter_host(home, store, project)?;
    fields.retain(|(name, _)| name != "project");
    fields.push(("project".to_owned(), SdkValue::text(project)));
    layerfs_home::trace::step("store_open");
    let outcome =
        crate::service::call_streamed(method, &open, SdkValue::Map(fields), running.owner, source);
    layerfs_home::trace::step("call");
    drop(running);
    outcome
}

fn call(op: &str, open: &OpenArgs, args: SdkValue, running: Running) -> Outcome {
    layerfs_home::trace::step("store_open");
    // An admitting call reads the bounded places this home can speak for,
    // once; no other call reads the home (`place_view::admits`). The engine
    // never opens a home record; a worker call installs its own view before
    // this, and that view is kept.
    let outcome = match crate::place_view::admits(op) {
        true => {
            crate::place_view::scoped(open, || direct::call_owned(op, open, args, running.owner))
        }
        false => direct::call_owned(op, open, args, running.owner),
    };
    layerfs_home::trace::step("call");
    drop(running);
    outcome
}

/// One call running on an open Store; its drop ends it.
struct Running {
    path: PathBuf,
    owner: Owner,
}

impl Drop for Running {
    fn drop(&mut self) {
        // The current host route admits only reads. A foreign writer can
        // have WAL frames even though this service has no writer to settle.
        let logged = self.owner == Owner::Direct && has_log(&self.path);
        let args = {
            let mut open = cache();
            let Some(entry) = open.get_mut(&self.path).and_then(Cached::ready) else {
                return;
            };
            entry.calls -= 1;
            CHANGED.notify_all();
            entry.last = Instant::now();
            match logged {
                true => settle(entry),
                false => None,
            }
        };
        let Some(args) = args else {
            return;
        };
        // the call is answered first: its checkpoint runs on its own thread
        let path = self.path.clone();
        let spawned = std::thread::Builder::new()
            .name("layerfs-store-checkpoint".to_owned())
            .spawn(move || {
                direct::transition(|| checkpoint(&args, false), |result| settled(&path, result));
            });
        if let Err(error) = spawned {
            settled(&self.path, Err(failed(error)));
        }
    }
}

/// Whether this process holds the Store at `path` open now.
pub(crate) fn holds(path: &Path) -> bool {
    cache().contains_key(path)
}

/// Starts an idle checkpoint on `entry` when it is the last user here: no
/// call and no other checkpoint running. Its open arguments.
fn settle(entry: &mut Entry) -> Option<OpenArgs> {
    if entry.calls > 0 || entry.settling > 0 {
        return None;
    }
    entry.settling += 1;
    Some(entry.args.clone())
}

/// Ends an idle checkpoint started by [`settle`] on the Store at `path`.
fn settled(path: &Path, result: Result<Option<bool>, SdkRefusal>) {
    let mut open = cache();
    if let Some(entry) = open.get_mut(path).and_then(Cached::ready) {
        entry.settling -= 1;
        entry.checkpoint = result;
    }
    CHANGED.notify_all();
}

/// Whether the Store file at `path` has a non-empty write-ahead log. A
/// PostgreSQL Store's key names no file: never.
fn has_log(path: &Path) -> bool {
    wal_remaining(path) == Some(true)
}

fn wal_remaining(path: &Path) -> Option<bool> {
    let mut log = path.as_os_str().to_os_string();
    log.push("-wal");
    match std::fs::symlink_metadata(log) {
        Ok(metadata) => Some(metadata.len() > 0),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Some(false),
        Err(_) => None,
    }
}

/// One `wal_checkpoint(TRUNCATE)` on the open Store `args` name, through
/// its open service. `Some(true)`: its log is empty; `Some(false)`: left for
/// the idle close; `None`: no writer available or the checkpoint failed.
/// An explicit Sandbox release can start a writer to complete its checkpoint.
fn checkpoint(args: &OpenArgs, explicit: bool) -> Result<Option<bool>, SdkRefusal> {
    direct::with_service(args, |service| {
        service.opened.checkpoint(explicit).map_err(failed)
    })
}

/// A Sandbox on the Store at `path` has closed: with no call running on it
/// here, its log is checkpointed and truncated once now, before the close
/// is answered (#86). The Store stays open. What [`checkpoint`] returns, or
/// `None` when nothing ran.
pub fn released(path: &Path) -> Option<bool> {
    if !has_log(path) {
        return None;
    }
    let args = {
        let mut open = cache();
        let cached = open.get_mut(path)?;
        if cached.host.is_some() {
            return None;
        }
        settle(cached.ready()?)?
    };
    direct::transition(
        || checkpoint(&args, true),
        |result| {
            let done = result.as_ref().ok().copied().flatten();
            settled(path, result);
            done
        },
    )
}

/// Opens the Store `args` name (`storePath`) in this process if it is not
/// open yet, `lease` held while it stays open: Store methods and the host
/// Sandbox code that reads a Branch through it both find it. Its path.
pub fn hold(args: &OpenArgs, lease: Option<Lease>) -> Result<PathBuf, SdkRefusal> {
    enter(args, lease, 0)
}

/// [`hold`], counting `calls` more calls running on it.
fn enter(args: &OpenArgs, lease: Option<Lease>, calls: usize) -> Result<PathBuf, SdkRefusal> {
    let path = args.store_path()?;
    let pending = loop {
        let mut open = cache();
        if open.get(&path).is_some_and(|cached| cached.host.is_some()) {
            return Err(in_use(&path, "host"));
        }
        match open.get_mut(&path).map(|cached| &mut cached.state) {
            Some(Slot::Ready(entry)) => {
                entry.admit(calls);
                return Ok(path);
            }
            Some(Slot::Opening(pending)) => {
                let pending = pending.clone();
                drop(open);
                pending.wait()?;
            }
            Some(Slot::Closing(pending)) => {
                let pending = pending.clone();
                drop(open);
                pending.wait()?;
            }
            None => break reserve(&mut open, path.clone(), None),
        }
    };
    opened(&path, args, calls, Owner::Direct, pending, || Ok(lease))?;
    Ok(path)
}

fn reserve(open: &mut Cache, path: PathBuf, host: Option<PathBuf>) -> Arc<Completion<()>> {
    let pending = Arc::new(Completion::new());
    open.insert(
        path,
        Cached {
            host,
            state: Slot::Opening(pending.clone()),
        },
    );
    pending
}

fn opened(
    path: &Path,
    args: &OpenArgs,
    calls: usize,
    owner: Owner,
    pending: Arc<Completion<()>>,
    claim: impl FnOnce() -> Result<Option<Lease>, SdkRefusal>,
) -> Result<(), SdkRefusal> {
    direct::transition(
        || {
            let lease = claim()?;
            if let Err(refusal) = direct::acquire(args, owner) {
                return Err(match release_lease(lease, owner, args) {
                    Ok(()) => refusal,
                    Err(release) => {
                        with_next(refusal, "lease_release", SdkValue::text(release.message))
                    }
                });
            }
            Ok(Entry {
                args: args.clone(),
                last: Instant::now(),
                calls,
                settling: 0,
                checkpoint: Ok(None),
                lease,
            })
        },
        |result| {
            let mut open = cache();
            let result = match result {
                Ok(entry) => {
                    open.get_mut(path).expect("open reserved").state = Slot::Ready(entry);
                    Ok(())
                }
                Err(refusal) => {
                    open.remove(path);
                    Err(refusal)
                }
            };
            CHANGED.notify_all();
            pending.finish(result)
        },
    )?;
    reaper();
    Ok(())
}

/// Number of host cache slots in use, including opens and closes in progress.
pub fn host_store_count(home: &Path) -> usize {
    count_host(&cache(), home)
}

fn count_host(open: &Cache, home: &Path) -> usize {
    open.values()
        .filter(|slot| slot.host.as_deref() == Some(home))
        .count()
}

fn enter_host(home: &Path, store: &str, project: &str) -> Result<(OpenArgs, Running), SdkRefusal> {
    let path = paths::store_file(home, store);
    crate::identity::store_dir(home, store)?;
    loop {
        let record_lock = record::lock(home, store)?;
        let record = StoreRecord::of(home, store)?;
        record.unblocked()?;
        if record.op().is_some_and(|op| op.name() == "store.compact") {
            record.unheld()?;
        }
        let StoreRef::Sqlite { machine, .. } = record.store_ref()? else {
            return Err(refused(
                "Unsupported",
                "host worker Store calls require local SQLite",
            ));
        };
        layerfs_machine::record::MachineRecord::admit_host(home, machine)?;
        if layerfs_machine::in_process() != Some(machine) {
            return Err(refused(
                "Unavailable",
                format!("Store {store} is placed on {machine}"),
            ));
        }
        let mut open = cache();
        if let Some(cached) = open.get_mut(&path) {
            match &mut cached.state {
                Slot::Ready(entry) => {
                    check_identity(&entry.args, &record)?;
                    if cached.host.is_some() {
                        entry.admit(1);
                        let args = OpenArgs::in_root(home, store, None).with_project(project);
                        return Ok((
                            args,
                            Running {
                                path,
                                owner: Owner::Host,
                            },
                        ));
                    }
                    // A direct owner cannot be displaced while it holds this
                    // path. A dedicated host worker acquires its own lease.
                    return Err(refused(
                        "Unavailable",
                        format!("Store {store} is already open on the direct route"),
                    ));
                }
                Slot::Opening(pending) => {
                    let pending = pending.clone();
                    drop(open);
                    drop(record_lock);
                    pending.wait()?;
                }
                Slot::Closing(pending) => {
                    let pending = pending.clone();
                    drop(open);
                    drop(record_lock);
                    pending.wait()?;
                }
            }
        } else if count_host(&open, home) < HOST_STORES {
            let pending = reserve(&mut open, path.clone(), Some(home.to_path_buf()));
            drop(open);
            let args = OpenArgs::in_root(home, store, Some(record.to_record().clone()))
                .with_project(project);
            opened(&path, &args, 1, Owner::Host, pending, || {
                lease::holders(home, store)?;
                let lease = lease::hold_host_locked(home, store)?;
                drop(record_lock);
                Ok(Some(lease))
            })?;
            return Ok((
                args,
                Running {
                    path,
                    owner: Owner::Host,
                },
            ));
        } else {
            let victim = open
                .iter()
                .filter_map(|(path, slot)| match &slot.state {
                    Slot::Ready(entry)
                        if slot.host.as_deref() == Some(home)
                            && entry.calls == 0
                            && entry.settling == 0 =>
                    {
                        Some((path, entry.last))
                    }
                    _ => None,
                })
                .min_by_key(|&(_, last)| last)
                .map(|(path, _)| path.clone());
            drop(record_lock);
            if let Some(victim) = victim {
                let closing = start_close(&mut open, &victim);
                drop(open);
                let _ = finish_idle(&victim, closing);
            } else {
                wait_changed(open);
            }
        }
    }
}

fn check_identity(args: &OpenArgs, record: &StoreRecord) -> Result<(), SdkRefusal> {
    let opened = args
        .store_record
        .as_ref()
        .expect("host admission supplied a record");
    let same = ["binding", "cursor"]
        .into_iter()
        .all(|key| opened.get(key) == record.field(key));
    if same {
        Ok(())
    } else {
        Err(refused(
            "Unavailable",
            format!("Store {} changed identity while open", record.name()),
        ))
    }
}

/// Starts the reaper thread once per process: every 5 s it closes the open
/// Stores, the PostgreSQL host sessions ([`crate::postgresql::reader`]) and
/// a host worker's held PostgreSQL read sessions
/// ([`crate::postgresql::pool`]) unused for [`IDLE`].
pub fn reaper() {
    REAPER.call_once(|| {
        std::thread::spawn(|| loop {
            std::thread::sleep(Duration::from_secs(5));
            close_idle(IDLE);
            crate::postgresql::reader::close_idle(IDLE);
            crate::postgresql::pool::close_idle(IDLE);
        });
    });
}

/// The close acknowledgment and its last idle-checkpoint result. WAL
/// presence is an observation after release: another opener can keep it.
#[derive(Clone)]
pub struct CloseReport {
    pub closed: bool,
    pub checkpoint: Result<Option<bool>, SdkRefusal>,
    pub wal_remaining: Option<bool>,
}

impl CloseReport {
    fn absent() -> Self {
        Self {
            closed: false,
            checkpoint: Ok(None),
            wal_remaining: None,
        }
    }
}

struct Closing {
    entry: Entry,
    owner: Owner,
    pending: Arc<Completion<CloseReport>>,
}

fn start_close(open: &mut Cache, path: &Path) -> Closing {
    let cached = open.get_mut(path).expect("ready while locked");
    let pending = Arc::new(Completion::new());
    let Slot::Ready(entry) = std::mem::replace(&mut cached.state, Slot::Closing(pending.clone()))
    else {
        unreachable!("ready while locked");
    };
    Closing {
        entry,
        owner: if cached.host.is_some() {
            Owner::Host
        } else {
            Owner::Direct
        },
        pending,
    }
}

fn finish_close(
    path: &Path,
    Closing {
        entry,
        owner,
        pending,
    }: Closing,
) -> Result<CloseReport, SdkRefusal> {
    direct::transition(
        || {
            let closed = direct::release(&entry.args, owner);
            let released = release_lease(entry.lease, owner, &entry.args);
            closed.and(released)?;
            Ok(CloseReport {
                closed: true,
                checkpoint: entry.checkpoint,
                wal_remaining: if entry.args.postgresql.is_none() {
                    wal_remaining(path)
                } else {
                    None
                },
            })
        },
        |result| {
            cache().remove(path);
            CHANGED.notify_all();
            pending.finish(result)
        },
    )
}

fn release_lease(lease: Option<Lease>, owner: Owner, args: &OpenArgs) -> Result<(), SdkRefusal> {
    match owner {
        Owner::Direct => {
            drop(lease);
            Ok(())
        }
        Owner::Host => {
            lease
                .ok_or_else(|| refused("Failed", "the host Store entry lost its lease"))?
                .release()?;
            let home = args.home.as_deref().expect("host lease home");
            let store = args.store.as_deref().expect("host lease Store");
            if lease::held_here(home, store) {
                return Err(in_use(&paths::store_file(home, store), "leased"));
            }
            Ok(())
        }
    }
}

fn finish_idle(path: &Path, closing: Closing) -> Result<CloseReport, SdkRefusal> {
    let host = closing.owner == Owner::Host;
    let outcome = finish_close(path, closing);
    if host {
        let mut log = io::stderr().lock();
        let _ = match &outcome {
            Ok(report) => writeln!(
                log,
                "layerfs host worker: Store {} closed: checkpoint={:?}, wal_remaining={:?}",
                path.display(),
                report.checkpoint,
                report.wal_remaining
            ),
            Err(refusal) => writeln!(
                log,
                "layerfs host worker: Store {} close failed: {}: {}",
                path.display(),
                refusal.code,
                refusal.message
            ),
        };
    }
    outcome
}

/// Closes every Store unused for at least `idle` with no call running.
/// An idle checkpoint running on one ends first.
pub fn close_idle(idle: Duration) -> usize {
    close_idle_report(idle).len()
}

/// As in [`close_idle`], with one outcome per settled Store. A final drain
/// also awaits entries whose physical close is already underway, and
/// closes every idle held PostgreSQL read session.
pub fn close_idle_report(idle: Duration) -> Vec<(PathBuf, Result<CloseReport, SdkRefusal>)> {
    if idle.is_zero() {
        crate::postgresql::pool::close_idle(Duration::ZERO);
    }
    let names: Vec<_> = cache()
        .iter()
        .filter_map(|(path, cached)| match &cached.state {
            Slot::Ready(entry) if is_idle(entry, idle) => Some(path.clone()),
            Slot::Opening(_) | Slot::Closing(_) if idle.is_zero() => Some(path.clone()),
            _ => None,
        })
        .collect();
    names
        .into_iter()
        .filter_map(|path| {
            let result = close_if_idle(&path, idle)?;
            Some((path, result))
        })
        .collect()
}

fn is_idle(entry: &Entry, idle: Duration) -> bool {
    entry.calls == 0 && entry.last.elapsed() >= idle
}

fn close_if_idle(path: &Path, idle: Duration) -> Option<Result<CloseReport, SdkRefusal>> {
    loop {
        let mut open = cache();
        match &open.get(path)?.state {
            Slot::Ready(entry) if is_idle(entry, idle) => {
                if entry.settling > 0 {
                    wait_changed(open);
                } else {
                    let closing = start_close(&mut open, path);
                    drop(open);
                    return Some(finish_idle(path, closing));
                }
            }
            Slot::Opening(pending) if idle.is_zero() => {
                let pending = pending.clone();
                drop(open);
                if pending.wait().is_err() {
                    return None;
                }
            }
            Slot::Closing(pending) if idle.is_zero() => {
                let pending = pending.clone();
                drop(open);
                return Some(pending.wait());
            }
            _ => return None,
        }
    }
}

/// Releases the cached Store at `path`, if present, and reports whether an
/// entry closed. Move and remove need its file closed.
pub fn close(path: &Path) -> Result<bool, SdkRefusal> {
    close_report(path).map(|report| report.closed)
}

/// Releases the cached Store at `path` and reports its checkpoint and close.
pub fn close_report(path: &Path) -> Result<CloseReport, SdkRefusal> {
    crate::postgresql::reader::forget(path);
    loop {
        let mut open = cache();
        match open.get(path).map(|cached| &cached.state) {
            None => return Ok(CloseReport::absent()),
            Some(Slot::Opening(_)) => return Err(in_use(path, "opening")),
            Some(Slot::Ready(entry)) if entry.calls > 0 => return Err(in_use(path, "active")),
            Some(Slot::Ready(entry)) if entry.settling > 0 => wait_changed(open),
            Some(Slot::Closing(pending)) => {
                let pending = pending.clone();
                drop(open);
                return pending.wait();
            }
            Some(Slot::Ready(_)) => {
                let closing = start_close(&mut open, path);
                drop(open);
                return finish_close(path, closing);
            }
        }
    }
}

/// Validates a host close against the authoritative record under the same
/// lock as admission, then fences this cache entry before unlocking.
pub fn close_host(
    home: &Path,
    store: &str,
    check: impl FnOnce(&StoreRecord) -> Result<(), SdkRefusal>,
) -> Result<CloseReport, SdkRefusal> {
    crate::identity::store_dir(home, store)?;
    let path = paths::store_file(home, store);
    let guard = record::lock_for_host_close(home, store)?;
    check(&StoreRecord::of(home, store)?)?;
    let mut open = cache();
    let Some(cached) = open.get(&path) else {
        return Ok(CloseReport::absent());
    };
    let state = match &cached.state {
        Slot::Opening(_) => "opening",
        Slot::Closing(_) => "closing",
        Slot::Ready(entry) if entry.calls > 0 => "active",
        Slot::Ready(entry) if entry.settling > 0 => "checkpointing",
        Slot::Ready(_) if cached.host.is_none() => "direct",
        Slot::Ready(_) => {
            let closing = start_close(&mut open, &path);
            drop(open);
            drop(guard);
            return finish_close(&path, closing);
        }
    };
    Err(with_next(
        in_use(&path, state),
        "store",
        SdkValue::text(store),
    ))
}

fn in_use(path: &Path, state: &str) -> SdkRefusal {
    let refusal = refused(
        "InUse",
        format!("Store {} is {state} in this process", path.display()),
    );
    with_next(refusal, "state", SdkValue::text(state))
}
