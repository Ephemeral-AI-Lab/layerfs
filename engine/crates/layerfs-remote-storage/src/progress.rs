//! Live progress of one transfer (#506 E4): its current phase, the bytes
//! done of the phase's total and, for a file stored as pieces, how many
//! pieces are done.
//!
//! A transfer runs on one thread. Its caller installs a [`Progress`] for
//! that thread with [`observe`]; the hashing loops, the request body and
//! reply loops and the piece loop report to it through [`begin`], [`add`]
//! and [`pieces`], which do nothing when none is installed. No thread is
//! added and nothing is retried. [`Cell`] keeps one sample that a reader on
//! another thread copies, published at most once per interval for bytes;
//! a new phase or piece count is published when it happens.
use std::cell::RefCell;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// How often a [`Cell`] publishes byte counts in production.
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

/// One published sample.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Sample {
    pub phase: Option<&'static str>,
    /// Bytes done in the phase; `None` in a phase without a byte total.
    pub bytes: Option<u64>,
    pub total: Option<u64>,
    /// `(done, of)` while a file goes up or down as pieces.
    pub pieces: Option<(u64, u64)>,
}

struct State {
    shown: Sample,
    bytes: u64,
    published: Instant,
}

/// A progress sample shared between the transfer's thread and readers.
pub struct Cell {
    interval: Duration,
    state: Mutex<State>,
}

impl Cell {
    /// A cell that publishes byte counts at most once per `interval`
    /// ([`INTERVAL`] in production).
    pub fn new(interval: Duration) -> Self {
        Self {
            interval,
            state: Mutex::new(State {
                shown: Sample::default(),
                bytes: 0,
                published: Instant::now(),
            }),
        }
    }

    fn state(&self) -> std::sync::MutexGuard<'_, State> {
        self.state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
    }

    /// The last published sample.
    pub fn sample(&self) -> Sample {
        self.state().shown.clone()
    }
}

impl Progress for Cell {
    fn begin(&self, phase: &'static str, total: Option<u64>) {
        let mut state = self.state();
        state.bytes = 0;
        state.published = Instant::now();
        state.shown = Sample {
            phase: Some(phase),
            bytes: total.map(|_| 0),
            total,
            pieces: None,
        };
    }

    fn add(&self, bytes: u64) {
        let mut state = self.state();
        state.bytes = state.bytes.saturating_add(bytes);
        if state.published.elapsed() >= self.interval {
            state.published = Instant::now();
            state.shown.bytes = Some(state.bytes);
        }
    }

    fn pieces(&self, done: u64, of: u64) {
        self.state().shown.pieces = Some((done, of));
    }
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
