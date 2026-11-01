//! The worker's idle stop (H §7.2, #533 step 13, O2): with no open Store, no
//! Sandbox, no call in flight and no call for [`IDLE`], the worker stops
//! serving; its front end then exits, and in a VM guest powers it off. The
//! host notices at its next call (the worker does not answer) and starts
//! the machine again: a readiness wait, not a retry.
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

/// O2: a machine's idle time.
pub const IDLE: Duration = Duration::from_secs(600);

static IN_FLIGHT: AtomicUsize = AtomicUsize::new(0);

/// One call being answered; dropping it ends the call.
pub(crate) struct Busy;

impl Busy {
    pub(crate) fn start() -> Self {
        IN_FLIGHT.fetch_add(1, Ordering::SeqCst);
        Busy
    }
}

impl Drop for Busy {
    fn drop(&mut self) {
        IN_FLIGHT.fetch_sub(1, Ordering::SeqCst);
        super::server::touch();
    }
}

/// Whether the worker at `root` has been idle for `idle` now.
pub fn idle(root: &Path, idle: Duration, started_ms: u64) -> bool {
    if IN_FLIGHT.load(Ordering::SeqCst) > 0 || crate::store::open::count() > 0 {
        return false;
    }
    let sandboxes = crate::home::paths::machine_dir(root, "native").join("sandbox");
    if crate::home::paths::names(&sandboxes)
        .map(|n| !n.is_empty())
        .unwrap_or(true)
    {
        return false;
    }
    let last = super::server::last_active().max(started_ms);
    let now = crate::home::now_ms() as u64;
    now.saturating_sub(last) >= idle.as_millis() as u64
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
    let started = crate::home::now_ms() as u64;
    let looked = root.clone();
    let server = std::thread::spawn(move || super::server::serve(listener, secret, root));
    loop {
        std::thread::sleep(every);
        if server.is_finished() {
            return match server.join() {
                Ok(result) => result.map(|_| ()),
                Err(_) => Err(std::io::Error::other("the worker's server panicked")),
            };
        }
        if idle(&looked, idle_for, started) {
            return Ok(());
        }
    }
}
