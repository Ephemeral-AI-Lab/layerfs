//! A progress sample shared between a transfer's thread and readers, for
//! the tests (moved out of the product, CUT): published at most once per
//! interval for bytes; a new phase or piece count is published when it
//! happens.
#![allow(dead_code)]
use layerfs_store::backup::bucket::progress::Progress;
use std::sync::Mutex;
use std::time::{Duration, Instant};

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
