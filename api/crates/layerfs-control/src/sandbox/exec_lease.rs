//! An exec's lease on its Sandbox (#548 B5): while a command runs, the
//! Sandbox's record holds a lease with a deadline. A close refuses to kill
//! while a lease is live; a crashed exec's lease expires and the Sandbox
//! becomes `Lost`, never a blind kill.
//!
//! The lease is refreshed by a small heartbeat thread so a command that
//! runs longer than one lease period stays live. Dropping the guard stops
//! the thread and removes the lease. A process that dies leaves the lease
//! behind, and the deadline turns it into `Lost`.
use crate::common::clock;
use crate::common::home;
use crate::common::outcome::refused;
use crate::sandbox::lifecycle;
use crate::sandbox::record::{self, SandboxRecord};
use layerfs_bridge::control::SdkRefusal;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

type Result<T> = std::result::Result<T, SdkRefusal>;

/// A running exec's lease on Sandbox `name` of `home`.
pub struct ExecLease {
    home: PathBuf,
    name: String,
    id: String,
    stop: Arc<AtomicBool>,
    wake: Option<Sender<()>>,
    heartbeat: Option<JoinHandle<()>>,
}

impl ExecLease {
    /// Takes a lease on Sandbox `name` of `home`, or refuses when a close
    /// holds the Sandbox, it is `Lost`/`Draining`, or its record is gone.
    pub fn begin(home: &Path, name: &str) -> Result<Self> {
        let ttl = ttl_ms();
        let id = lease_id()?;
        let _home = home::lock::take(home)?;
        let _sandbox = record::lock(home, name)?;
        let Some(record) = SandboxRecord::read(home, name)? else {
            return Err(refused("NoSuchSandbox", format!("no Sandbox named {name}")));
        };
        record.free()?;
        if record.lost() {
            return Err(lifecycle::lost(name, "the Sandbox is Lost"));
        }
        if record.draining() {
            return Err(refused(
                "Busy",
                format!("Sandbox {name} is draining; it admits no new exec"),
            ));
        }
        let now = home::now_ms();
        let activity = clock::monotonic_ms();
        let record = record.with_exec_lease(&id, now.saturating_add(ttl), activity);
        record.write(home)?;
        let stop = Arc::new(AtomicBool::new(false));
        let (wake, sleep) = channel();
        let heartbeat = heartbeat(
            home.to_owned(),
            name.to_owned(),
            id.clone(),
            Arc::clone(&stop),
            sleep,
            ttl,
        );
        Ok(Self {
            home: home.to_owned(),
            name: name.to_owned(),
            id,
            stop,
            wake: Some(wake),
            heartbeat,
        })
    }
}

impl Drop for ExecLease {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(wake) = self.wake.take() {
            let _ = wake.send(());
        }
        if let Some(heartbeat) = self.heartbeat.take() {
            let _ = heartbeat.join();
        }
        let _ = release(&self.home, &self.name, &self.id);
    }
}

/// The period of one lease, from `LAYERFS_EXEC_LEASE_SECS` (default 900).
fn ttl_ms() -> i64 {
    std::env::var("LAYERFS_EXEC_LEASE_SECS")
        .ok()
        .and_then(|seconds| seconds.parse::<i64>().ok())
        .filter(|seconds| *seconds > 0)
        .map(|seconds| seconds.saturating_mul(1000))
        .unwrap_or(lifecycle::EXEC_LEASE_MS)
}

/// A fresh `lease-<16 hex>`.
fn lease_id() -> Result<String> {
    let bytes = crate::sandbox::registry::random()?;
    let id = u64::from_be_bytes(bytes[..8].try_into().expect("eight bytes"));
    Ok(format!("lease-{id:016x}"))
}

/// Refresh `id`'s deadline every third of its period until `stop`.
fn heartbeat(
    home: PathBuf,
    name: String,
    id: String,
    stop: Arc<AtomicBool>,
    sleep: Receiver<()>,
    ttl: i64,
) -> Option<JoinHandle<()>> {
    let pause = Duration::from_millis(u64::try_from((ttl / 3).max(1000)).unwrap_or(1000));
    std::thread::Builder::new()
        .name("layerfs-exec-lease".to_owned())
        .spawn(move || loop {
            match sleep.recv_timeout(pause) {
                Ok(()) | Err(RecvTimeoutError::Disconnected) => break,
                Err(RecvTimeoutError::Timeout) => {}
            }
            if stop.load(Ordering::Relaxed) {
                break;
            }
            if refresh(&home, &name, &id, ttl).is_err() {
                break;
            }
        })
        .ok()
}

/// Extend lease `id` to `now + ttl`; a record that no longer holds it
/// ends the heartbeat.
fn refresh(home: &Path, name: &str, id: &str, ttl: i64) -> Result<()> {
    let _home = home::lock::take(home)?;
    let _sandbox = record::lock(home, name)?;
    let Some(record) = SandboxRecord::read(home, name)? else {
        return Err(refused("NoSuchSandbox", format!("no Sandbox named {name}")));
    };
    if !record.has_lease(id) {
        return Err(lifecycle::lost(name, "the exec lease was removed"));
    }
    let record = record.with_exec_lease(
        id,
        home::now_ms().saturating_add(ttl),
        clock::monotonic_ms(),
    );
    record.write(home)
}

/// Remove lease `id` from Sandbox `name`'s record; a closed Sandbox is
/// already gone and needs nothing.
fn release(home: &Path, name: &str, id: &str) -> Result<()> {
    let _home = home::lock::take(home)?;
    let _sandbox = record::lock(home, name)?;
    let Some(record) = SandboxRecord::read(home, name)? else {
        return Ok(());
    };
    if !record.has_lease(id) {
        return Ok(());
    }
    let record = record
        .without_exec_lease(id)
        .with_activity(clock::monotonic_ms());
    record.write(home)
}
