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
//! the call is answered; after `sandbox.close` it runs before that answer, so
//! a `store status` right after sees the truncated log. A checkpoint that
//! cannot complete now (another process writing, a reader still in the log)
//! is left for the idle close and never retried.
use crate::lease::Lease;
use crate::service as direct;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_home::outcome::{refused, Outcome};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Condvar, Mutex, Once};
use std::time::{Duration, Instant};

/// How long an unused Store stays open.
pub const IDLE: Duration = Duration::from_secs(60);

struct Entry {
    /// The arguments it was opened with: what `direct::release` needs.
    args: SdkValue,
    last: Instant,
    /// Calls running on it now.
    calls: usize,
    /// Idle checkpoints running on it now (#86); it stays open meanwhile.
    settling: usize,
    _lease: Option<Lease>,
}

static OPEN: Mutex<BTreeMap<PathBuf, Entry>> = Mutex::new(BTreeMap::new());
static REAPER: Once = Once::new();
/// Signalled when an idle checkpoint ends: `close` waits for it.
static SETTLED: Condvar = Condvar::new();

fn store_path(args: &SdkValue) -> Result<PathBuf, SdkRefusal> {
    crate::identity::store_path(args)
}

/// Runs Store method `op` once on the Store `args` name (`storePath`),
/// opening it first if this process holds it not yet. `lease` is held for
/// as long as the Store stays open. The Store stays open while it runs.
pub fn run(op: &str, args: SdkValue, lease: Option<Lease>) -> Outcome {
    let running = Running(enter(&args, lease, 1)?);
    let outcome = direct::call(op, args);
    drop(running);
    outcome
}

/// One call running on an open Store; its drop ends it.
struct Running(PathBuf);

impl Drop for Running {
    fn drop(&mut self) {
        let logged = has_log(&self.0);
        let args = {
            let mut open = OPEN.lock().unwrap_or_else(|e| e.into_inner());
            let Some(entry) = open.get_mut(&self.0) else {
                return;
            };
            entry.calls = entry.calls.saturating_sub(1);
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
        let path = self.0.clone();
        let spawned = std::thread::Builder::new()
            .name("layerfs-store-checkpoint".to_owned())
            .spawn(move || {
                let _ = checkpoint(&args);
                settled(&path);
            });
        if spawned.is_err() {
            settled(&self.0);
        }
    }
}

/// Starts an idle checkpoint on `entry` when it is the last user here: no
/// call and no other checkpoint running. Its open arguments.
fn settle(entry: &mut Entry) -> Option<SdkValue> {
    if entry.calls > 0 || entry.settling > 0 {
        return None;
    }
    entry.settling += 1;
    Some(entry.args.clone())
}

/// Ends an idle checkpoint started by [`settle`] on the Store at `path`.
fn settled(path: &Path) {
    let mut open = OPEN.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(entry) = open.get_mut(path) {
        entry.settling = entry.settling.saturating_sub(1);
    }
    SETTLED.notify_all();
}

/// Whether the Store file at `path` has a non-empty write-ahead log. A
/// PostgreSQL Store's key names no file: never.
fn has_log(path: &Path) -> bool {
    let mut log = path.as_os_str().to_os_string();
    log.push("-wal");
    std::fs::symlink_metadata(log).is_ok_and(|metadata| metadata.len() > 0)
}

/// One `wal_checkpoint(TRUNCATE)` on the open Store `args` name, through
/// its open service. `Some(true)`: its log is empty; `Some(false)`: left for
/// the idle close; `None`: no Store file (PostgreSQL) or it failed.
fn checkpoint(args: &SdkValue) -> Option<bool> {
    direct::with_service(args, |service| {
        Ok(service.opened.file().map(|file| file.checkpoint()))
    })
    .ok()
    .flatten()
    .and_then(Result::ok)
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
        let mut open = OPEN.lock().unwrap_or_else(|e| e.into_inner());
        settle(open.get_mut(path)?)?
    };
    let done = checkpoint(&args);
    settled(path);
    done
}

/// Opens the Store `args` name (`storePath`) in this process if it is not
/// open yet, `lease` held while it stays open: Store methods and the host
/// Sandbox code that reads a Branch through it both find it. Its path.
pub fn hold(args: &SdkValue, lease: Option<Lease>) -> Result<PathBuf, SdkRefusal> {
    enter(args, lease, 0)
}

/// [`hold`], counting `calls` more calls running on it.
fn enter(args: &SdkValue, lease: Option<Lease>, calls: usize) -> Result<PathBuf, SdkRefusal> {
    let path = store_path(args)?;
    let mut open = OPEN.lock().unwrap_or_else(|e| e.into_inner());
    match open.get_mut(&path) {
        Some(entry) => {
            entry.last = Instant::now();
            entry.calls += calls;
        }
        None => {
            direct::acquire(args)?;
            open.insert(
                path.clone(),
                Entry {
                    args: args.clone(),
                    last: Instant::now(),
                    calls,
                    settling: 0,
                    _lease: lease,
                },
            );
            reaper();
        }
    }
    Ok(path)
}

/// Starts the reaper thread once per process: every 5 s it closes the open
/// Stores and the PostgreSQL host sessions ([`crate::postgresql::reader`]) unused for
/// [`IDLE`].
pub fn reaper() {
    REAPER.call_once(|| {
        std::thread::spawn(|| loop {
            std::thread::sleep(Duration::from_secs(5));
            close_idle(IDLE);
            crate::postgresql::reader::close_idle(IDLE);
        });
    });
}

/// Closes every Store unused for at least `idle` with no call running;
/// returns how many. An idle checkpoint running on one ends first.
pub fn close_idle(idle: Duration) -> usize {
    let unused = |entry: &Entry| entry.calls == 0 && entry.last.elapsed() >= idle;
    let stale: Vec<Entry> = {
        let mut open = OPEN.lock().unwrap_or_else(|e| e.into_inner());
        // An idle checkpoint on a Store to close (one statement, started
        // when its last call returned) ends first, never skipping the
        // Store: a process that closes its Stores before it exits (the CLI,
        // `close_idle(ZERO)`) must still close each one, so SQLite folds the
        // write-ahead log in at that close, as before #86.
        while open
            .values()
            .any(|entry| unused(entry) && entry.settling > 0)
        {
            open = SETTLED.wait(open).unwrap_or_else(|e| e.into_inner());
        }
        let names: Vec<PathBuf> = open
            .iter()
            .filter(|(_, entry)| unused(entry))
            .map(|(path, _)| path.clone())
            .collect();
        names.iter().filter_map(|path| open.remove(path)).collect()
    };
    let count = stale.len();
    for entry in stale {
        // the Store closes when its last call returns; the lease drops after
        let _ = direct::release(&entry.args);
    }
    count
}

/// Closes the Store at `path` now, if this process holds it open: `store
/// move` and `store remove` need its file closed. A call running on it is
/// `InUse`, and the Store stays open. An idle checkpoint running on it
/// (one statement) ends first.
pub fn close(path: &std::path::Path) -> Result<bool, SdkRefusal> {
    crate::postgresql::reader::forget(path);
    let entry = {
        let mut open = OPEN.lock().unwrap_or_else(|e| e.into_inner());
        while open.get(path).is_some_and(|entry| entry.settling > 0) {
            open = SETTLED.wait(open).unwrap_or_else(|e| e.into_inner());
        }
        match open.get(path) {
            Some(entry) if entry.calls > 0 => {
                return Err(refused(
                    "InUse",
                    format!("a call is running on {} in this process", path.display()),
                ))
            }
            _ => open.remove(path),
        }
    };
    match entry {
        Some(entry) => direct::release(&entry.args).map(|_| true),
        None => Ok(false),
    }
}
