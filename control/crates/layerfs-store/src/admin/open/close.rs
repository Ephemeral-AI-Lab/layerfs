//! The reaper and every close of the open-Store cache ([`super::cache`]):
//! the idle close, an explicit close and the close report.
use super::cache::*;
use super::checkpoint::wal_remaining;
use crate::admin::open::args::OpenArgs;
use crate::admin::open::lease::{self, Lease};
use crate::admin::open::service::{self as direct, Completion, Route, Slot};
use layerfs_home::disk::paths;
use layerfs_home::reply::outcome::{refused, with_next};
use layerfs_protocol::sdk::{SdkRefusal, SdkValue};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

/// Starts the reaper thread once per process: every 5 s it closes the open
/// Stores, the PostgreSQL host sessions ([`crate::backend::postgresql::reader`]) and
/// a host worker's held PostgreSQL read sessions
/// ([`crate::backend::postgresql::pool`]) unused for [`IDLE`].
pub fn reaper() {
    REAPER.call_once(|| {
        std::thread::spawn(|| loop {
            std::thread::sleep(Duration::from_secs(5));
            close_idle(IDLE);
            crate::backend::postgresql::reader::close_idle(IDLE);
            crate::backend::postgresql::pool::close_idle(IDLE);
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
    pub(super) fn absent() -> Self {
        Self {
            closed: false,
            checkpoint: Ok(None),
            wal_remaining: None,
        }
    }
}

pub(super) struct Closing {
    pub(super) entry: Entry,
    pub(super) owner: Route,
    pub(super) pending: Arc<Completion<CloseReport>>,
}

pub(super) fn start_close(open: &mut Cache, path: &Path) -> Closing {
    let cached = open.get_mut(path).expect("ready while locked");
    let pending = Arc::new(Completion::new());
    let Slot::Ready(entry) = std::mem::replace(&mut cached.state, Slot::Closing(pending.clone()))
    else {
        unreachable!("ready while locked");
    };
    Closing {
        entry,
        owner: if cached.host.is_some() {
            Route::Host
        } else {
            Route::Direct
        },
        pending,
    }
}

pub(super) fn finish_close(
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

pub(super) fn release_lease(
    lease: Option<Lease>,
    owner: Route,
    args: &OpenArgs,
) -> Result<(), SdkRefusal> {
    match owner {
        Route::Direct => {
            drop(lease);
            Ok(())
        }
        Route::Host => {
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

pub(super) fn finish_idle(path: &Path, closing: Closing) -> Result<CloseReport, SdkRefusal> {
    let host = closing.owner == Route::Host;
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
/// closes every idle held PostgreSQL session, read or write.
pub fn close_idle_report(idle: Duration) -> Vec<(PathBuf, Result<CloseReport, SdkRefusal>)> {
    if idle.is_zero() {
        crate::backend::postgresql::pool::close_idle(Duration::ZERO);
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

pub(super) fn is_idle(entry: &Entry, idle: Duration) -> bool {
    entry.calls == 0 && entry.last.elapsed() >= idle
}

pub(super) fn close_if_idle(
    path: &Path,
    idle: Duration,
) -> Option<Result<CloseReport, SdkRefusal>> {
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
    crate::backend::postgresql::reader::forget(path);
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

pub(super) fn in_use(path: &Path, state: &str) -> SdkRefusal {
    let refusal = refused(
        "InUse",
        format!("Store {} is {state} in this process", path.display()),
    );
    with_next(refusal, "state", SdkValue::text(state))
}
