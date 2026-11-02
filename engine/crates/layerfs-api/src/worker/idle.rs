//! The worker's idle stop (H §7.2, #533 step 13, O2): with no open Store, no
//! Sandbox (live, or recorded by the host Sandbox code), no call in flight and no call for [`IDLE`], the worker stops
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
    if sandboxes_held(root) {
        return false;
    }
    let last = super::server::last_active().max(started_ms);
    let now = crate::home::now_ms() as u64;
    now.saturating_sub(last) >= idle.as_millis() as u64
}

/// Whether a Sandbox lives under the worker's root: one this process runs,
/// or one the host Sandbox code recorded at
/// `store/<s>/project/<p>/sandboxes/<n>/` (its daemon may outlive a worker
/// restart). An unreadable folder counts as one, so the guest stays up.
fn sandboxes_held(root: &Path) -> bool {
    use crate::home::paths;
    if crate::host::registry::live_sandboxes() > 0 {
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
