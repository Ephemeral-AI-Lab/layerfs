//! The host worker's background start (#833).
//!
//! An ordinary call that finds no ready worker (no endpoint, a refused
//! connect, a socket not yet private, or another caller starting one) runs
//! on its direct route, chosen before anything is sent to a worker, and
//! starts the worker without waiting, so the next call finds it ready. The
//! start takes the startup lock without waiting and hands it to the worker
//! as its stdin: the worker keeps it until its endpoints are published, so
//! concurrent first calls start one worker, and one that dies first frees
//! it. A start that fails never fails the call: its reason is in the trace
//! and in [`start_failure`]'s record, which doctor reports. The caller
//! writes a pending marker before it spawns; a worker that ends before it
//! publishes, even by a signal, leaves the marker with the startup lock
//! free, which [`start_failure`] reports as such.
use super::super::host_diagnostic::free_lock;
use super::{existing_socket, hold, probe, HostPaths, Probe, Reached};
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_home::home::{self, paths};
use layerfs_home::trace;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::process::Child;
use std::time::Instant;

/// The longest failure message the record keeps, in bytes.
pub const START_FAILURE_LIMIT: usize = 1024;

/// The route reason of a call whose worker another caller is starting.
const STARTING: &str = "worker starting";
/// The route reason of a call whose worker holds its lifetime lock with no
/// endpoint published yet (starting without the startup lock, or exiting).
const HELD: &str = "worker not ready";
/// The route reason of the call that started the worker.
const STARTED: &str = "worker started";
/// What a background start whose worker ended before publishing, leaving
/// no record of its own (a signal, or an exit before it could write one),
/// reports.
const ENDED: &str = "the worker ended before publishing its endpoints (killed, or exited without a record); see log/worker.log";

/// The record's `launch`: an ordinary call's background start, or
/// `machine start`, which waited for the worker and was answered itself.
pub const LAUNCH_BACKGROUND: &str = "background";
/// See [`LAUNCH_BACKGROUND`].
pub const LAUNCH_MACHINE_START: &str = "machine start";

impl HostPaths {
    /// The route of an ordinary call that found no ready worker at `path`:
    /// direct, after starting the worker unless another caller is starting
    /// it, or the worker's stream when a start finished meanwhile. A host
    /// machine stopped meanwhile is today's refusal.
    pub(super) fn start_detached(&self, path: &Path, end: Instant) -> Result<Reached, SdkRefusal> {
        let ready = || match probe(path, end)? {
            Probe::Connected(stream) => Ok(Some(stream)),
            _ => Ok(None),
        };
        Ok(match self.start_unless(ready)? {
            Ok(stream) => Reached::Stream(stream, None, false),
            Err(reason) => Reached::Direct(reason),
        })
    }

    /// Starts the worker in the background under the startup lock unless
    /// `ready`, asked once that lock is held, finds it ready (`Ok` with what
    /// it found); otherwise the direct route's reason (`Err`).
    fn start_unless<T>(
        &self,
        ready: impl FnOnce() -> Result<Option<T>, SdkRefusal>,
    ) -> Result<Result<T, String>, SdkRefusal> {
        let startup = match hold(&paths::host_worker_startup_lock(&self.home)) {
            Ok(Some(lock)) => lock,
            Ok(None) => return Ok(Err(STARTING.to_owned())),
            Err(refusal) => return Ok(Err(self.start_failed(&refusal))),
        };
        // a start that finished while this caller took the lock
        if let Some(found) = ready()? {
            return Ok(Ok(found));
        }
        match self.try_hold() {
            Ok(Some(unowned)) => drop(unowned),
            Ok(None) => return Ok(Err(HELD.to_owned())),
            Err(refusal) => return Ok(Err(self.start_failed(&refusal))),
        }
        crate::record::MachineRecord::admit_host(&self.home, layerfs_home::names::HOST_MACHINE)?;
        // From here this start is the one doctor reports: an earlier
        // failure is superseded, and the marker outlives a worker that
        // dies before it publishes.
        let pending = SdkValue::map([
            ("at", SdkValue::Int(home::now_ms())),
            ("pid", SdkValue::Int(i64::from(std::process::id()))),
        ]);
        if let Err(refusal) = home::write_record(&self.pending(), &pending) {
            return Ok(Err(self.start_failed(&refusal)));
        }
        let _ = std::fs::remove_file(paths::host_worker_start_failure(&self.home));
        match self.spawn(Some(startup)) {
            Ok(child) => {
                reap(child);
                Ok(Err(STARTED.to_owned()))
            }
            Err(refusal) => Ok(Err(self.start_failed(&refusal))),
        }
    }

    /// Whether streamed write `op` goes direct because no worker's write
    /// endpoint is published, decided as a background start decides (and
    /// starting the worker), but before the caller copies a piped input
    /// into its spool, which a direct write would only read back. It never
    /// dials: the write dials an endpoint published meanwhile itself.
    pub fn direct_unless_published(&self, op: &str) -> Result<bool, SdkRefusal> {
        let published = || {
            Ok(existing_socket(&self.writes)?
                .is_some_and(|metadata| metadata.mode() & 0o777 == 0o600))
        };
        if published()? {
            return Ok(false);
        }
        self.check_paths()?;
        crate::record::MachineRecord::admit_host(&self.home, layerfs_home::names::HOST_MACHINE)?;
        match self.start_unless(|| Ok(published()?.then_some(())))? {
            Ok(()) => Ok(false),
            Err(reason) => {
                trace::step("background_start");
                trace::route(&self.writes.to_string_lossy(), op, "direct", &reason);
                Ok(true)
            }
        }
    }

    fn pending(&self) -> PathBuf {
        paths::host_worker_start_pending(&self.home)
    }

    /// Removes a pending marker an earlier background start left, before
    /// `machine start` spawns a worker it waits for, so that worker's
    /// failure is recorded as `machine start`'s.
    pub(super) fn forget_pending_start(&self) {
        let _ = std::fs::remove_file(self.pending());
    }

    /// Records a failed background start and names it as the route reason;
    /// the start has ended, so its pending marker goes.
    fn start_failed(&self, refusal: &SdkRefusal) -> String {
        record_start_failure(&self.home, "caller", LAUNCH_BACKGROUND, &refusal.message);
        self.forget_pending_start();
        format!("worker start failed: {}", refusal.message)
    }

    /// Records that this worker failed before publishing its endpoints
    /// (`message`); best effort. See [`record_worker_start_failure`].
    pub fn record_start_failure(&self, message: &str) {
        record_worker_start_failure(&self.home, message);
    }

    /// Removes the record of an earlier failed start and the pending
    /// marker of this one, once this worker's endpoints are published;
    /// either not being there is fine.
    pub fn clear_start_failure(&self) {
        let _ = std::fs::remove_file(paths::host_worker_start_failure(&self.home));
        self.forget_pending_start();
    }
}

/// Records that the worker of `home` failed before publishing its
/// endpoints (`message`), also when it could not read its home's identity
/// or panicked; best effort. A pending marker says a background start
/// launched it; without one, `machine start` did.
pub fn record_worker_start_failure(home: &Path, message: &str) {
    let launch = match paths::host_worker_start_pending(home).exists() {
        true => LAUNCH_BACKGROUND,
        false => LAUNCH_MACHINE_START,
    };
    record_start_failure(home, "worker", launch, message);
}

fn record_start_failure(home: &Path, by: &str, launch: &str, message: &str) {
    let mut message = message.to_owned();
    if message.len() > START_FAILURE_LIMIT {
        let mut end = START_FAILURE_LIMIT;
        while !message.is_char_boundary(end) {
            end -= 1;
        }
        message.truncate(end);
    }
    let record = SdkValue::map([
        ("at", SdkValue::Int(home::now_ms())),
        ("by", SdkValue::text(by)),
        ("launch", SdkValue::text(launch)),
        ("pid", SdkValue::Int(i64::from(std::process::id()))),
        ("message", SdkValue::Text(message)),
    ]);
    let _ = home::write_record(&paths::host_worker_start_failure(home), &record);
}

/// The host worker's last failed start, if any: `{ at, by, launch, pid,
/// message }`. Its record, or else a background start's pending marker
/// whose worker ended without publishing (the startup lock is free),
/// reported with the starting caller's `pid`. Read-only: a lock file that
/// is not there is not made.
pub fn start_failure(home: &Path) -> Option<SdkValue> {
    if let Some(record) = home::read_record(&paths::host_worker_start_failure(home))
        .ok()
        .flatten()
    {
        return Some(record);
    }
    let pending = home::read_record(&paths::host_worker_start_pending(home))
        .ok()
        .flatten()?;
    // The caller holds the startup lock from before it writes the marker,
    // and its worker holds it until it publishes and removes the marker:
    // a held lock is a start in progress. The probe's lock is released
    // when dropped.
    let _free = free_lock(&paths::host_worker_startup_lock(home)).ok()??;
    Some(SdkValue::map([
        ("at", pending.get("at").cloned().unwrap_or_default()),
        ("by", SdkValue::text("worker")),
        ("launch", SdkValue::text(LAUNCH_BACKGROUND)),
        ("pid", pending.get("pid").cloned().unwrap_or_default()),
        ("message", SdkValue::text(ENDED)),
    ]))
}

/// The worker outlives its caller and is never waited for; a caller that
/// lives on reaps it when it ends.
fn reap(mut child: Child) {
    let _ = std::thread::Builder::new()
        .name("layerfs-host-reaper".to_owned())
        .spawn(move || {
            let _ = child.wait();
        });
}
