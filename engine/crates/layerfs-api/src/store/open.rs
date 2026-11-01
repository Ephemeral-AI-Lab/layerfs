//! The Stores this process holds open: a call opens its Store once and later
//! calls reuse it; a Store unused for [`IDLE`] is closed by a reaper thread
//! (H §7.7). The same cache serves the caller's in-process route and a
//! machine's worker. Each entry keeps what must live as long as the open
//! Store: the in-process route's lease.
use crate::host::direct;
use crate::host::outcome::Outcome;
use crate::store::lease::Lease;
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
    _lease: Option<Lease>,
}

static OPEN: Mutex<BTreeMap<PathBuf, Entry>> = Mutex::new(BTreeMap::new());
static REAPER: Once = Once::new();

fn store_path(args: &SdkValue) -> Result<PathBuf, SdkRefusal> {
    crate::host::stores::store_path(args)
}

/// Runs Store method `op` once on the Store `args` name (`storePath`),
/// opening it first if this process holds it not yet. `lease` is held for
/// as long as the Store stays open.
pub(crate) fn run(op: &str, args: SdkValue, lease: Option<Lease>) -> Outcome {
    let path = store_path(&args)?;
    {
        let mut open = OPEN.lock().unwrap_or_else(|e| e.into_inner());
        match open.get_mut(&path) {
            Some(entry) => entry.last = Instant::now(),
            None => {
                direct::acquire(&args)?;
                open.insert(
                    path.clone(),
                    Entry {
                        args: args.clone(),
                        last: Instant::now(),
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
    }
    let outcome = direct::call(op, args, false);
    if let Some(entry) = OPEN
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get_mut(&path)
    {
        entry.last = Instant::now();
    }
    outcome
}

/// Closes every Store unused for at least `idle`; returns how many.
pub(crate) fn close_idle(idle: Duration) -> usize {
    let stale: Vec<Entry> = {
        let mut open = OPEN.lock().unwrap_or_else(|e| e.into_inner());
        let names: Vec<PathBuf> = open
            .iter()
            .filter(|(_, entry)| entry.last.elapsed() >= idle)
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
