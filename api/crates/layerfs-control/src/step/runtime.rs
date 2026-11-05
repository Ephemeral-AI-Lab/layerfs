//! The step-runtime use cases (#555): start asynchronously, wait through
//! the durable entry, read status, cancel by id. The launcher owns the wait
//! and the deadline; the Sandbox daemon is not consulted and no in-memory
//! promise is the wait. A launcher restart re-reads the entry, so a wait
//! resumes from it; an outcome that cannot be known is recorded `Unresolved`
//! and never replayed.
use crate::step::contract::{CancelOutcome, ExecId, ExecReceipt, ExecRequest, ExecStatus, Started};
use crate::step::launcher::{Clock, Launcher, Observation};
use crate::step::store::{Claim, Entry, Transition, WaitStore};
use layerfs_bridge::control::SdkRefusal;
use std::time::Duration;

/// Milliseconds between two observations in one wait.
const POLL_MS: i64 = 25;

/// The step runtime: the launcher-owned helper that starts one command,
/// waits through the durable entry, answers its status and cancels it by id.
///
/// `L` is the launcher port and `C` the clock. The runtime holds no process
/// and spawns nothing itself: `exec_async` records the entry and returns, and
/// `exec_wait` polls the entry and the launcher's observation until a
/// terminal status or the caller's wait bound.
pub struct StepRuntime<L, C> {
    store: WaitStore,
    launcher: L,
    clock: C,
}

impl<L: Launcher, C: Clock> StepRuntime<L, C> {
    /// A runtime over `store`, starting commands through `launcher` and
    /// reading time from `clock`.
    pub fn new(store: WaitStore, launcher: L, clock: C) -> Self {
        Self {
            store,
            launcher,
            clock,
        }
    }

    /// Starts `request` and answers its durable id without waiting.
    ///
    /// A retry with the same idempotency key answers the id an earlier call
    /// made (`reused: true`) and starts no second copy. The command, the
    /// deadline and the key must match the first call or the retry is
    /// refused `Conflict`.
    pub fn exec_async(&self, request: ExecRequest) -> Result<Started, SdkRefusal> {
        let received = self.clock.now_ms();
        match self.store.claim(&request)? {
            Claim::New(id) => self.start_new(id, request, received),
            Claim::Reused(id) => {
                let status = self
                    .store
                    .read(&id)?
                    .map_or(ExecStatus::Unknown, |entry| entry.status());
                Ok(Started {
                    exec_id: id,
                    reused: true,
                    status,
                })
            }
        }
    }

    /// The status of `id` now, reconciling a passed deadline and a terminal
    /// observation into the durable entry.
    ///
    /// `Unknown` means no entry names the id. `Unresolved` means an entry
    /// exists but its outcome cannot be known; it is terminal and is never
    /// replayed.
    pub fn exec_status(&self, id: &ExecId) -> Result<ExecStatus, SdkRefusal> {
        let Some(entry) = self.store.read(id)? else {
            return Ok(ExecStatus::Unknown);
        };
        if entry.status().is_terminal() {
            return Ok(entry.status());
        }
        match self.launcher.observe(&entry.handle()) {
            Observation::Running if entry.deadline_passed(self.clock.now_ms()) => {
                self.record_timeout(id)
            }
            Observation::Running => Ok(ExecStatus::Running),
            Observation::Ended(code) => self.record_end(id, code),
            Observation::Gone => self.record(id, ExecStatus::Unresolved),
        }
    }

    /// Waits up to `wait` for `id` to reach a terminal status.
    ///
    /// It answers a terminal status as soon as it sees one, and `Running`
    /// when the wait bound passes first: the exec continues, and a later
    /// `exec_wait` or `exec_status` resumes from the same durable entry. A
    /// wait bound of zero observes once and returns.
    pub fn exec_wait(&self, id: &ExecId, wait: Duration) -> Result<ExecStatus, SdkRefusal> {
        let end = self
            .clock
            .now_ms()
            .saturating_add(i64::try_from(wait.as_millis()).unwrap_or(i64::MAX));
        loop {
            let status = self.exec_status(id)?;
            if status.is_terminal() {
                return Ok(status);
            }
            let now = self.clock.now_ms();
            if now >= end {
                return Ok(status);
            }
            let step = (end - now).clamp(1, POLL_MS);
            std::thread::sleep(Duration::from_millis(u64::try_from(step).unwrap_or(1)));
        }
    }

    /// Cancels `id` by its string identity.
    ///
    /// A cancellation recorded before the deadline answers `Cancelled`; a
    /// deadline that passed first answers `TimedOut`, and a later cancel
    /// keeps answering `TimedOut`. An id with no entry, an exec that already
    /// finished, a gone process and an entry that cannot be resolved answer
    /// `Unknown`: no cancellation was established.
    pub fn cancel(&self, id: &ExecId) -> Result<CancelOutcome, SdkRefusal> {
        let Some(entry) = self.store.read(id)? else {
            return Ok(CancelOutcome::Unknown);
        };
        if entry.status().is_terminal() {
            return Ok(cancel_outcome(entry.status()));
        }
        match self.launcher.observe(&entry.handle()) {
            Observation::Ended(code) => {
                self.record_end(id, code)?;
                return Ok(CancelOutcome::Unknown);
            }
            Observation::Gone => {
                self.record(id, ExecStatus::Unresolved)?;
                return Ok(CancelOutcome::Unknown);
            }
            Observation::Running => {}
        }
        // The deadline check and the terminal write happen under the same
        // per-entry lock, so a concurrent deadline and cancellation cannot
        // both win: whichever writes first fixes the outcome.
        let transition = self.store.transition(id, |current| {
            if current.status().is_terminal() {
                return None;
            }
            let now = self.clock.now_ms();
            let status = if current.deadline_passed(now) {
                ExecStatus::TimedOut
            } else {
                ExecStatus::Cancelled
            };
            Some(current.terminal(status, now))
        })?;
        let entry = match transition {
            Transition::Changed(entry) => {
                // Best effort: the outcome is already recorded, and a
                // process that does not answer the signal still did not
                // turn a cancellation into a success.
                let _ = self.launcher.cancel(&entry.handle());
                entry
            }
            Transition::Unchanged(entry) => entry,
            Transition::Missing => return Ok(CancelOutcome::Unknown),
        };
        Ok(cancel_outcome(entry.status()))
    }

    /// The durable receipt of `id`, or `None` when no entry names it.
    pub fn receipt(&self, id: &ExecId) -> Result<Option<ExecReceipt>, SdkRefusal> {
        self.store.receipt(id)
    }

    /// Starts one claimed id and records its running entry.
    fn start_new(&self, id: ExecId, request: ExecRequest, now: i64) -> Result<Started, SdkRefusal> {
        let deadline = request
            .timeout_ms
            .map(|ms| now.saturating_add(i64::try_from(ms).unwrap_or(i64::MAX)));
        let handle = match self.launcher.start(&id, &request) {
            Ok(handle) => handle,
            Err(error) => {
                // The start may have happened: record the unknown outcome
                // once so a retry never runs a second copy, then report it.
                let entry = Entry::unresolved(&id, &request, now);
                let _ = self.store.insert(&entry);
                return Err(error);
            }
        };
        let entry = Entry::running(&id, &request, handle, now, deadline);
        self.store.insert(&entry)?;
        Ok(Started {
            exec_id: id,
            reused: false,
            status: ExecStatus::Running,
        })
    }

    /// Records terminal `status` on `id` unless an earlier call fixed one.
    fn record(&self, id: &ExecId, status: ExecStatus) -> Result<ExecStatus, SdkRefusal> {
        let transition = self.store.transition(id, |entry| {
            if entry.status().is_terminal() {
                None
            } else {
                Some(entry.terminal(status, self.clock.now_ms()))
            }
        })?;
        Ok(match transition {
            Transition::Changed(entry) | Transition::Unchanged(entry) => entry.status(),
            Transition::Missing => ExecStatus::Unknown,
        })
    }

    /// Records `code` as `Succeeded` or `Failed` on `id`.
    fn record_end(&self, id: &ExecId, code: i32) -> Result<ExecStatus, SdkRefusal> {
        let status = if code == 0 {
            ExecStatus::Succeeded { exit_code: code }
        } else {
            ExecStatus::Failed { exit_code: code }
        };
        self.record(id, status)
    }

    /// Records a passed deadline as `TimedOut`, then asks the launcher to
    /// stop the command. A timeout is not a cancellation, even when the
    /// signal that ends the process is the same.
    fn record_timeout(&self, id: &ExecId) -> Result<ExecStatus, SdkRefusal> {
        let transition = self.store.transition(id, |entry| {
            let now = self.clock.now_ms();
            if entry.status().is_terminal() || !entry.deadline_passed(now) {
                None
            } else {
                Some(entry.terminal(ExecStatus::TimedOut, now))
            }
        })?;
        Ok(match transition {
            Transition::Changed(entry) => {
                let _ = self.launcher.cancel(&entry.handle());
                entry.status()
            }
            Transition::Unchanged(entry) => entry.status(),
            Transition::Missing => ExecStatus::Unknown,
        })
    }
}

/// The cancel answer a recorded status means.
fn cancel_outcome(status: ExecStatus) -> CancelOutcome {
    match status {
        ExecStatus::Cancelled => CancelOutcome::Cancelled,
        ExecStatus::TimedOut => CancelOutcome::TimedOut,
        _ => CancelOutcome::Unknown,
    }
}
