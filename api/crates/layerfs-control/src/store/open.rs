//! The Stores this process holds open: a call opens its Store once and later
//! calls reuse it; a Store unused for [`IDLE`] is closed by a reaper thread
//! (H §7.7). The same cache serves the caller's in-process route and a
//! machine's worker. Each entry keeps what must live as long as the open
//! Store: its lease. A Store with a call running is never closed: the
//! reaper skips it and `close` refuses `InUse`.
use crate::common::home::lease::Lease;
use crate::common::outcome::{refused, Outcome};
use crate::host::direct;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Mutex, Once};
use std::time::{Duration, Instant};

/// How long an unused Store stays open.
pub const IDLE: Duration = Duration::from_secs(60);

struct Entry {
    /// The arguments it was opened with: what `direct::release` needs.
    args: SdkValue,
    last: Instant,
    /// Calls running on it now.
    calls: usize,
    _lease: Option<Lease>,
}

static OPEN: Mutex<BTreeMap<PathBuf, Entry>> = Mutex::new(BTreeMap::new());
static REAPER: Once = Once::new();

fn store_path(args: &SdkValue) -> Result<PathBuf, SdkRefusal> {
    crate::host::stores::store_path(args)
}

/// Runs Store method `op` once on the Store `args` name (`storePath`),
/// opening it first if this process holds it not yet. `lease` is held for
/// as long as the Store stays open. The Store stays open while it runs.
pub(crate) fn run(op: &str, args: SdkValue, lease: Option<Lease>) -> Outcome {
    let running = Running(enter(&args, lease, 1)?);
    let outcome = direct::call(op, args, false);
    drop(running);
    outcome
}

/// One call running on an open Store; its drop ends it.
struct Running(PathBuf);

impl Drop for Running {
    fn drop(&mut self) {
        let mut open = OPEN.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(entry) = open.get_mut(&self.0) {
            entry.calls = entry.calls.saturating_sub(1);
            entry.last = Instant::now();
        }
    }
}

/// Opens the Store `args` name (`storePath`) in this process if it is not
/// open yet, `lease` held while it stays open: Store methods and the host
/// Sandbox code that reads a Branch through it both find it. Its path.
pub(crate) fn hold(args: &SdkValue, lease: Option<Lease>) -> Result<PathBuf, SdkRefusal> {
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
                    _lease: lease,
                },
            );
            REAPER.call_once(|| {
                std::thread::spawn(|| loop {
                    std::thread::sleep(Duration::from_secs(5));
                    close_idle(IDLE);
                });
            });
        }
    }
    Ok(path)
}

/// Closes every Store unused for at least `idle` with no call running;
/// returns how many.
pub(crate) fn close_idle(idle: Duration) -> usize {
    let stale: Vec<Entry> = {
        let mut open = OPEN.lock().unwrap_or_else(|e| e.into_inner());
        let names: Vec<PathBuf> = open
            .iter()
            .filter(|(_, entry)| entry.calls == 0 && entry.last.elapsed() >= idle)
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
/// `InUse`, and the Store stays open.
pub(crate) fn close(path: &std::path::Path) -> Result<bool, SdkRefusal> {
    let entry = {
        let mut open = OPEN.lock().unwrap_or_else(|e| e.into_inner());
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

/// How many Stores this process holds open.
pub(crate) fn count() -> usize {
    OPEN.lock().unwrap_or_else(|e| e.into_inner()).len()
}
