//! Live progress of one transfer (#506 E4): its current phase, the bytes
//! done of the phase's total and, for a file stored as pieces, how many
//! pieces are done.
//!
//! A transfer runs on one thread. Its caller installs a [`Progress`] for
//! that thread with [`observe`]; the hashing loops, the request body and
//! reply loops and the piece loop report to it through [`begin`], [`add`]
//! and [`pieces`], which do nothing when none is installed. No thread is
//! added and nothing is retried.
use std::cell::RefCell;
use std::sync::Arc;
use std::time::Duration;

/// How often a transfer publishes byte counts.
pub const INTERVAL: Duration = Duration::from_millis(250);

/// Where a transfer reports its progress.
pub trait Progress: Send + Sync {
    /// A new phase starts, with its total bytes when known.
    fn begin(&self, phase: &'static str, total: Option<u64>);
    /// `bytes` more are done in the current phase.
    fn add(&self, bytes: u64);
    /// `done` of `of` pieces are done.
    fn pieces(&self, done: u64, of: u64);
}

thread_local! {
    static CURRENT: RefCell<Option<Arc<dyn Progress>>> = const { RefCell::new(None) };
}

/// Puts back the progress a scope replaced when it ends.
struct Restore(Option<Arc<dyn Progress>>);

impl Drop for Restore {
    fn drop(&mut self) {
        let previous = self.0.take();
        CURRENT.with(|current| *current.borrow_mut() = previous);
    }
}

/// Runs `work` with `progress` receiving this thread's reports; the
/// previous one, if any, is restored after.
pub fn observe<T>(progress: Arc<dyn Progress>, work: impl FnOnce() -> T) -> T {
    let _restore = Restore(CURRENT.with(|current| current.borrow_mut().replace(progress)));
    work()
}

/// Runs `work` with no progress on this thread; the current one, if any,
/// is restored after.
pub fn quiet<T>(work: impl FnOnce() -> T) -> T {
    let _restore = Restore(CURRENT.with(|current| current.borrow_mut().take()));
    work()
}

fn with(report: impl FnOnce(&dyn Progress)) {
    let current = CURRENT.with(|current| current.borrow().clone());
    if let Some(progress) = current {
        report(progress.as_ref());
    }
}

/// Starts `phase` on this thread's progress, if any.
pub fn begin(phase: &'static str, total: Option<u64>) {
    with(|progress| progress.begin(phase, total));
}

/// Adds `bytes` to this thread's current phase, if any.
pub fn add(bytes: u64) {
    with(|progress| progress.add(bytes));
}

/// Reports `done` of `of` pieces to this thread's progress, if any.
pub fn pieces(done: u64, of: u64) {
    with(|progress| progress.pieces(done, of));
}
