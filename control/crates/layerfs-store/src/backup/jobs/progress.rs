//! Live progress of `backup.create` and `backup.restore`: the call's own
//! phases and the bucket library's byte counts, sent as the one public
//! event `{ phase, done, total }` (call surface 2.0). `done` and `total`
//! are bytes while a phase moves bytes (`total` null when unknown) and null
//! in a phase that moves none; how many pieces a file goes up or down as is
//! not reported. An update is sent only when it differs from the last
//! one: byte counts at most once per [`INTERVAL`] while a transfer runs,
//! and each phase's exact last count before the next phase. A phase that
//! reaches its total says only that its bytes moved; the call has
//! succeeded when it returns its result, never before.
use crate::backup::bucket::progress::{observe, Progress, INTERVAL};
use layerfs_home::events::Events;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex, MutexGuard};

/// One progress update.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct Update {
    phase: &'static str,
    done: Option<u64>,
    total: Option<u64>,
}

impl Update {
    fn send(self, events: &mut Events<'_>) {
        let count = |n: Option<u64>| n.map(|n| n.min(i64::MAX as u64) as i64);
        events.phase(self.phase, count(self.done), count(self.total), vec![]);
    }
}

/// What a transfer reported since it was last read: the updates that
/// ended a phase, in order, and the current one.
#[derive(Default)]
struct Reports {
    kept: Vec<Update>,
    current: Option<Update>,
}

/// Receives the bucket library's reports on the transfer's thread.
#[derive(Default)]
struct Recorder(Mutex<Reports>);

impl Recorder {
    fn reports(&self) -> MutexGuard<'_, Reports> {
        self.0.lock().unwrap_or_else(|poison| poison.into_inner())
    }

    /// The updates since the last take, ending with the current one.
    fn take(&self) -> Vec<Update> {
        let mut reports = self.reports();
        let mut out = std::mem::take(&mut reports.kept);
        out.extend(reports.current);
        out
    }
}

impl Progress for Recorder {
    fn begin(&self, phase: &'static str, total: Option<u64>) {
        let mut reports = self.reports();
        if let Some(ended) = reports.current.take() {
            reports.kept.push(ended);
        }
        reports.current = Some(Update {
            phase,
            done: total.map(|_| 0),
            total,
        });
    }

    fn add(&self, bytes: u64) {
        if let Some(current) = &mut self.reports().current {
            current.done = Some(current.done.unwrap_or(0).saturating_add(bytes));
        }
    }

    /// Pieces are not reported (call surface 2.0).
    fn pieces(&self, _done: u64, _of: u64) {}
}

/// The progress one call sends: each update once, in order.
#[derive(Default)]
pub struct Shown {
    last: Option<Update>,
}

impl Shown {
    pub fn new() -> Self {
        Self::default()
    }

    fn send(&mut self, events: &mut Events<'_>, update: Update) {
        if self.last != Some(update) {
            update.send(events);
            self.last = Some(update);
        }
    }

    /// Enters `phase` of the call's own, which moves no bytes.
    pub fn phase(&mut self, events: &mut Events<'_>, phase: &'static str) {
        self.send(
            events,
            Update {
                phase,
                ..Update::default()
            },
        );
    }

    /// `done` of `total` bytes of the call's own `phase` have moved.
    pub fn bytes(
        &mut self,
        events: &mut Events<'_>,
        phase: &'static str,
        done: u64,
        total: Option<u64>,
    ) {
        self.send(
            events,
            Update {
                phase,
                done: Some(done),
                total,
            },
        );
    }

    /// Runs `work`, one transfer through the bucket library, on a thread
    /// of its own that reports to this call; this thread sends the
    /// reports every [`INTERVAL`] and once more when `work` ends, however
    /// it ends. A panic in `work` is resumed here. Nothing is retried.
    pub fn transfer<T: Send>(
        &mut self,
        events: &mut Events<'_>,
        work: impl FnOnce() -> T + Send,
    ) -> T {
        let recorder = Arc::new(Recorder::default());
        let reporting: Arc<dyn Progress> = recorder.clone();
        std::thread::scope(|scope| {
            // dropped when `work` returns or unwinds
            let (running, ended) = mpsc::channel::<()>();
            let worker = scope.spawn(move || {
                let _running = running;
                observe(reporting, work)
            });
            while let Err(RecvTimeoutError::Timeout) = ended.recv_timeout(INTERVAL) {
                for update in recorder.take() {
                    self.send(events, update);
                }
            }
            for update in recorder.take() {
                self.send(events, update);
            }
            worker
                .join()
                .unwrap_or_else(|panic| std::panic::resume_unwind(panic))
        })
    }
}
