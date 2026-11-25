//! The process-wide cancel request (#804 §6): the `layerfs` command's signal
//! thread sets it on the first SIGINT or SIGTERM during `store upgrade`, and
//! the upgrade reads it until its `commit` phase starts. The call clears
//! it once its answer is decided ([`clear`]), so a long-lived process (an
//! SDK host) is not left cancelling every later call.
use std::sync::atomic::{AtomicBool, Ordering};

static REQUESTED: AtomicBool = AtomicBool::new(false);

/// The flag itself, for a call that hands it to the engine.
pub fn flag() -> &'static AtomicBool {
    &REQUESTED
}

/// Asks the running call to stop.
pub fn request() {
    REQUESTED.store(true, Ordering::SeqCst);
}

/// Whether a cancel was asked for.
pub fn requested() -> bool {
    REQUESTED.load(Ordering::SeqCst)
}

/// Drops a cancel request: the call it was for has answered.
pub fn clear() {
    REQUESTED.store(false, Ordering::SeqCst);
}
