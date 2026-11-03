//! The worker's idle stop (H §7.2, #533 step 13, O2): with no open Store, no
//! Sandbox (live, or recorded by the host Sandbox code), no running
//! Firecracker microVM it hosts, no connection open past its secret and no
//! call for [`IDLE`], the worker stops serving. A call is an authenticated
//! request for an operation that does work (`store.call`, `store.file`,
//! `sandbox.host`, `project.remove`): a connection, the secret
//! alone, a ping, an unknown operation, or a denied or malformed frame is a
//! probe and does not restart the clock. Its front end then exits, and in a
//! VM guest powers it off. The host notices at its next call (the worker
//! does not answer) and starts the machine again: a readiness wait, not a
//! retry.
//!
//! The stop decision and the admission of an authenticated connection take
//! one lock ([`Gate`]): a connection admitted before the decision holds the
//! worker up until it closes, so its request is read and answered; one
//! that comes after is refused `Unavailable` before its request is sent. A
//! probe closes at once, so it does not hold the worker up.
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::Duration;

/// O2: a machine's idle time.
pub const IDLE: Duration = Duration::from_secs(600);

static IN_FLIGHT: AtomicUsize = AtomicUsize::new(0);

/// One connection open past its secret; dropping it closes it. It holds the
/// worker up but does not restart the idle clock.
pub(crate) struct Busy;

impl Drop for Busy {
    fn drop(&mut self) {
        IN_FLIGHT.fetch_sub(1, Ordering::SeqCst);
    }
}

/// One server's stop decision, taken under the same lock as each
/// connection's admission.
#[derive(Default)]
pub(crate) struct Gate {
    stopping: Mutex<bool>,
}

impl Gate {
    /// Admits a connection whose secret matched, unless the worker has
    /// decided to stop.
    pub(crate) fn admit(&self) -> Option<Busy> {
        let stopping = self.stopping.lock().unwrap_or_else(|e| e.into_inner());
        (!*stopping).then(|| {
            IN_FLIGHT.fetch_add(1, Ordering::SeqCst);
            Busy
        })
    }

    /// Decides to stop when the worker at `root` is idle: no connection is
    /// admitted after that.
    fn stop_if_idle(&self, root: &Path, idle_for: Duration, started_ms: u64) -> bool {
        let mut stopping = self.stopping.lock().unwrap_or_else(|e| e.into_inner());
        if !*stopping && idle(root, idle_for, started_ms) {
            *stopping = true;
        }
        *stopping
    }
}

/// Whether the worker at `root` has been idle for `idle` now.
pub fn idle(root: &Path, idle: Duration, started_ms: u64) -> bool {
    if IN_FLIGHT.load(Ordering::SeqCst) > 0 || crate::store::open::count() > 0 {
        return false;
    }
    if sandboxes_held(root) || microvms_running(root) {
        return false;
    }
    let last = super::server::last_active().max(started_ms);
    let now = crate::common::home::now_ms() as u64;
    now.saturating_sub(last) >= idle.as_millis() as u64
}

/// Whether a Sandbox lives under the worker's root: one this process runs,
/// or one the host Sandbox code recorded at
/// `store/<s>/project/<p>/sandboxes/<n>/` (its daemon may outlive a worker
/// restart). An unreadable folder counts as one, so the guest stays up.
fn sandboxes_held(root: &Path) -> bool {
    use crate::common::home::paths;
    if crate::sandbox::registry::live_sandboxes() > 0 {
        return true;
    }
    let Ok(stores) = paths::names(&root.join("store")) else {
        return true;
    };
    for store in stores {
        let Ok(projects) = paths::names(&paths::store_dir(root, &store).join("project")) else {
            return true;
        };
        for project in projects {
            let sandboxes = paths::project_dir(root, &store, &project).join("sandboxes");
            match paths::names(&sandboxes) {
                Ok(names) if names.is_empty() => {}
                _ => return true,
            }
        }
    }
    false
}

/// Whether a Firecracker microVM this guest hosts still runs: a live
/// `firecracker/vms/<vm>/firecracker.pid` under the root (a Lima host's
/// data disk). Powering the host off would stop it under its own worker.
/// An unreadable folder or pid file counts as one running.
fn microvms_running(root: &Path) -> bool {
    let vms = root.join("firecracker/vms");
    let Ok(names) = crate::common::home::paths::names(&vms) else {
        return true;
    };
    names.iter().any(
        |vm| match std::fs::read_to_string(vms.join(vm).join("firecracker.pid")) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => false,
            Err(_) => true,
            Ok(text) => text
                .trim()
                .parse::<u32>()
                .map_or(true, crate::common::home::lease::alive),
        },
    )
}

/// Serves on `listener` on its own thread until the worker has been idle
/// for `idle`, looking every `every`; returns then.
pub fn serve_until_idle(
    listener: std::net::TcpListener,
    secret: String,
    root: std::path::PathBuf,
    idle_for: Duration,
    every: Duration,
) -> std::io::Result<()> {
    let started = crate::common::home::now_ms() as u64;
    let looked = root.clone();
    let gate = std::sync::Arc::new(Gate::default());
    let served = gate.clone();
    let server =
        std::thread::spawn(move || super::server::serve_gated(listener, secret, root, served));
    loop {
        std::thread::sleep(every);
        if server.is_finished() {
            return match server.join() {
                Ok(result) => result.map(|_| ()),
                Err(_) => Err(std::io::Error::other("the worker's server panicked")),
            };
        }
        if gate.stop_if_idle(&looked, idle_for, started) {
            return Ok(());
        }
    }
}

/// Powers this Linux guest off with `reboot(2)`, after a sync: the idle
/// stop of a guest without a `poweroff` program, such as a microsandbox
/// microVM, whose libkrun VMM exits on it. Needs `CAP_SYS_BOOT` in the
/// initial PID namespace; answers why it could not.
#[cfg(target_os = "linux")]
pub fn power_off() -> std::io::Error {
    nix::unistd::sync();
    match nix::sys::reboot::reboot(nix::sys::reboot::RebootMode::RB_POWER_OFF) {
        Ok(never) => match never {},
        Err(errno) => std::io::Error::from(errno),
    }
}
