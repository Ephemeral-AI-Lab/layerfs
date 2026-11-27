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
use super::checkpoint::{checkpoint, has_log, settle, settled};
use super::close::{in_use, reaper, release_lease, CloseReport};
use crate::admin::open::args::OpenArgs;
use crate::admin::open::lease::Lease;
use crate::admin::open::service::{self as direct, Completion, Route, Slot};
use layerfs_home::outcome::{failed, with_next, Outcome};
use layerfs_protocol::sdk::{SdkRefusal, SdkValue};
use std::cell::Cell;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, Once};
use std::time::{Duration, Instant};

/// How long an unused Store stays open.
pub const IDLE: Duration = Duration::from_secs(60);

pub(super) struct Entry {
    /// What it was opened with: what `direct::release` needs.
    pub(super) args: OpenArgs,
    pub(super) last: Instant,
    /// Calls running on it now.
    pub(super) calls: usize,
    /// Idle checkpoints running on it now (#86); it stays open meanwhile.
    pub(super) settling: usize,
    pub(super) checkpoint: Result<Option<bool>, SdkRefusal>,
    pub(super) lease: Option<Lease>,
}

pub(super) struct Cached {
    pub(super) host: Option<PathBuf>,
    pub(super) state: Slot<Entry, CloseReport>,
}

pub(super) type Cache = BTreeMap<PathBuf, Cached>;
pub(super) static OPEN: Mutex<Cache> = Mutex::new(BTreeMap::new());
pub(super) static REAPER: Once = Once::new();
/// Idle checkpoints and capacity waiters share these state notifications.
pub(super) static CHANGED: Condvar = Condvar::new();

pub(super) fn cache() -> MutexGuard<'static, Cache> {
    OPEN.lock().unwrap_or_else(|e| e.into_inner())
}

pub(super) fn wait_changed(open: MutexGuard<'static, Cache>) {
    drop(CHANGED.wait(open).unwrap_or_else(|e| e.into_inner()));
}

impl Cached {
    pub(super) fn ready(&mut self) -> Option<&mut Entry> {
        match &mut self.state {
            Slot::Ready(entry) => Some(entry),
            _ => None,
        }
    }
}

impl Entry {
    pub(super) fn admit(&mut self, calls: usize) {
        self.last = Instant::now();
        self.calls += calls;
    }
}

thread_local! {
    pub(super) static ONE_SHOT: Cell<bool> = const { Cell::new(false) };
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
        owner: Route::Direct,
    };
    call(op, open, args, running)
}

pub(super) fn call(op: &str, open: &OpenArgs, args: SdkValue, running: Running) -> Outcome {
    layerfs_home::trace::step("store_open");
    // An admitting call reads the bounded places this home can speak for,
    // once; no other call reads the home (`place_view::admits`). The engine
    // never opens a home record; a worker call installs its own view before
    // this, and that view is kept.
    let outcome = match crate::admin::route::place_view::admits(op) {
        true => crate::admin::route::place_view::scoped(open, || {
            direct::call_owned(op, open, args, running.owner)
        }),
        false => direct::call_owned(op, open, args, running.owner),
    };
    layerfs_home::trace::step("call");
    drop(running);
    outcome
}

/// One call running on an open Store; its drop ends it.
pub(super) struct Running {
    pub(super) path: PathBuf,
    pub(super) owner: Route,
}

impl Drop for Running {
    fn drop(&mut self) {
        // The current host route admits only reads. A foreign writer can
        // have WAL frames even though this service has no writer to settle.
        let logged = self.owner == Route::Direct && has_log(&self.path);
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

/// Opens the Store `args` name (`storePath`) in this process if it is not
/// open yet, `lease` held while it stays open: Store methods and the host
/// Sandbox code that reads a Branch through it both find it. Its path.
pub fn hold(args: &OpenArgs, lease: Option<Lease>) -> Result<PathBuf, SdkRefusal> {
    enter(args, lease, 0)
}

/// [`hold`], counting `calls` more calls running on it.
pub(super) fn enter(
    args: &OpenArgs,
    lease: Option<Lease>,
    calls: usize,
) -> Result<PathBuf, SdkRefusal> {
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
    opened(&path, args, calls, Route::Direct, pending, || Ok(lease))?;
    Ok(path)
}

pub(super) fn reserve(
    open: &mut Cache,
    path: PathBuf,
    host: Option<PathBuf>,
) -> Arc<Completion<()>> {
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

pub(super) fn opened(
    path: &Path,
    args: &OpenArgs,
    calls: usize,
    owner: Route,
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
