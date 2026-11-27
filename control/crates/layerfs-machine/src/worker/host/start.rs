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
//! and in [`start_failure`]'s record, which doctor reports.
use super::{existing_socket, hold, probe, HostPaths, Probe, Reached, READY_WAIT};
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_home::home::{self, paths};
use layerfs_home::trace;
use std::os::unix::fs::MetadataExt;
use std::path::Path;
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

impl HostPaths {
    /// The route of an ordinary call that found no ready worker at `path`:
    /// direct, after starting the worker unless another caller is starting
    /// it, or the worker's stream when a start finished meanwhile. A host
    /// machine stopped meanwhile is today's refusal.
    pub(super) fn start_detached(&self, path: &Path, end: Instant) -> Result<Reached, SdkRefusal> {
        let startup = match hold(&paths::host_worker_startup_lock(&self.home)) {
            Ok(Some(lock)) => lock,
            Ok(None) => return Ok(Reached::Direct(STARTING.to_owned())),
            Err(refusal) => return Ok(self.start_failed("caller", &refusal)),
        };
        // a start that finished while this caller took the lock
        if let Probe::Connected(stream) = probe(path, end)? {
            return Ok(Reached::Stream(stream, None, false));
        }
        match self.try_hold() {
            Ok(Some(unowned)) => drop(unowned),
            Ok(None) => return Ok(Reached::Direct(HELD.to_owned())),
            Err(refusal) => return Ok(self.start_failed("caller", &refusal)),
        }
        crate::record::MachineRecord::admit_host(&self.home, layerfs_home::names::HOST_MACHINE)?;
        match self.spawn(Some(startup)) {
            Ok(child) => {
                reap(child);
                Ok(Reached::Direct(STARTED.to_owned()))
            }
            Err(refusal) => Ok(self.start_failed("caller", &refusal)),
        }
    }

    /// Whether streamed write `op` goes direct because no worker's write
    /// endpoint is published, decided as a background start decides (and
    /// starting the worker), but before the caller copies a piped input
    /// into its spool, which a direct write would only read back.
    pub fn direct_unless_published(&self, op: &str) -> Result<bool, SdkRefusal> {
        let published =
            existing_socket(&self.writes)?.is_some_and(|metadata| metadata.mode() & 0o777 == 0o600);
        if published {
            return Ok(false);
        }
        self.check_paths()?;
        crate::record::MachineRecord::admit_host(&self.home, layerfs_home::names::HOST_MACHINE)?;
        match self.start_detached(&self.writes, Instant::now() + READY_WAIT)? {
            Reached::Direct(reason) => {
                trace::step("background_start");
                trace::route(&self.writes.to_string_lossy(), op, "direct", &reason);
                Ok(true)
            }
            // ready meanwhile: the write dials it again once spooled
            Reached::Stream(..) | Reached::Absent => Ok(false),
        }
    }

    /// Records a failed background start and names it as the route reason.
    fn start_failed(&self, by: &str, refusal: &SdkRefusal) -> Reached {
        record_start_failure(&self.home, by, &refusal.message);
        Reached::Direct(format!("worker start failed: {}", refusal.message))
    }

    /// Records that this worker failed before publishing its endpoints
    /// (`message`); best effort, under the startup lock its starter holds.
    pub fn record_start_failure(&self, message: &str) {
        record_start_failure(&self.home, "worker", message);
    }

    /// Removes the record of an earlier failed start, once this worker's
    /// endpoints are published; a record that is not there is fine.
    pub fn clear_start_failure(&self) {
        let _ = std::fs::remove_file(paths::host_worker_start_failure(&self.home));
    }
}

fn record_start_failure(home: &Path, by: &str, message: &str) {
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
        ("pid", SdkValue::Int(i64::from(std::process::id()))),
        ("message", SdkValue::Text(message)),
    ]);
    let _ = home::write_record(&paths::host_worker_start_failure(home), &record);
}

/// The record of the host worker's last failed background start, if any:
/// `{ at, by, pid, message }`.
pub fn start_failure(home: &Path) -> Option<SdkValue> {
    home::read_record(&paths::host_worker_start_failure(home))
        .ok()
        .flatten()
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
