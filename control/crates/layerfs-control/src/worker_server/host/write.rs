//! Host worker file writes: their own admission, the streamed Store write,
//! and the custody of outcomes their caller may not learn (§8.5).
//!
//! Fairness (§11): a `store.write` request, once read, leaves its ordinary
//! connection slot and thread and is held apart, at most [`WRITES_HELD`]
//! at once, for a lane of its own: [`WRITE_THREADS`] write threads. A
//! write never runs on an ordinary thread, so however many writes stream,
//! short calls keep every ordinary thread and slot. A held write waits for
//! a write thread while its input fits the socket buffer; its caller's
//! own send bound ends a wait that never gets one.
//!
//! Drain (§8.3) stops the input of every running write: a write whose
//! `end` frame was not read yet is then refused with nothing committed (its
//! input was never complete), and one past `end` finishes as admitted.
//!
//! Custody (§8.5): an engine-uncertain outcome is written to its record
//! before its answer is sent. If that write fails the worker holds the
//! outcome in memory and logs it, and it neither drains, exits idle nor
//! closes that Store until the record is written.
use super::{log, HostServer};
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_home::outcome::{refused, Outcome};
use layerfs_machine::worker::StreamReader;
use std::collections::VecDeque;
use std::net::Shutdown;
use std::os::unix::net::UnixStream;
use std::time::Instant;

/// Write threads: streamed writes that run at once.
pub(super) const WRITE_THREADS: usize = 4;
/// Streamed writes held at once, running or waiting, apart from the
/// ordinary connection slots: each holds one descriptor and no thread
/// while it waits.
pub(super) const WRITES_HELD: usize = 16;

/// An uncertain outcome whose record could not be written yet.
pub(super) struct Unkept {
    request_id: String,
    args: SdkValue,
    outcome: Outcome,
}

impl Unkept {
    /// Whether it is a write on `store`.
    pub(super) fn on(&self, store: &str) -> bool {
        self.args.get_text("store") == Some(store)
    }
}

/// One `store.write` request whose input still follows on its connection.
pub(in crate::worker_server) struct Held {
    pub(in crate::worker_server) stream: UnixStream,
    pub(in crate::worker_server) args: SdkValue,
    pub(in crate::worker_server) request_id: String,
    pub(in crate::worker_server) traced: bool,
}

/// The held writes of one worker, under its activity lock.
#[derive(Default)]
pub(super) struct Writes {
    held: usize,
    running: usize,
    waiting: VecDeque<Held>,
    /// The connections of running writes, whose input a drain stops.
    inputs: Vec<(u64, UnixStream)>,
    next: u64,
}

impl Writes {
    /// Held writes, running or waiting.
    pub(super) fn held(&self) -> usize {
        self.held
    }

    /// Running writes and waiting writes.
    pub(super) fn counts(&self) -> (usize, usize) {
        (self.running, self.waiting.len())
    }

    /// Ends the input of every running write (drain).
    pub(super) fn stop_inputs(&self) {
        for (_, input) in &self.inputs {
            let _ = input.shutdown(Shutdown::Read);
        }
    }
}

impl HostServer {
    /// Holds a write read on an ordinary connection. Past [`WRITES_HELD`]
    /// it is refused before any of its input is read: nothing committed.
    pub(in crate::worker_server) fn hold_write(&self, held: Held) {
        let mut activity = self.lock();
        if activity.writes.held >= WRITES_HELD && !activity.draining {
            drop(activity);
            let mut refusal = refused(
                "Unavailable",
                format!(
                    "the host worker already holds {WRITES_HELD} streamed writes; nothing was committed"
                ),
            );
            refusal.detail = SdkValue::map([
                ("request_id", SdkValue::text(&held.request_id)),
                ("writes", SdkValue::Int(WRITES_HELD as i64)),
            ]);
            super::super::connection::reply(&held.stream, &held.request_id, false, Err(refusal));
            return;
        }
        activity.writes.held += 1;
        activity.writes.waiting.push_back(held);
        drop(activity);
        self.writable.notify_one();
    }

    /// One write thread: runs held writes until the worker drains and none
    /// waits (a write taken while draining is refused at once).
    pub(super) fn work_writes(&self) {
        loop {
            let held = {
                let mut activity = self.lock();
                loop {
                    if let Some(held) = activity.writes.waiting.pop_front() {
                        activity.writes.running += 1;
                        break Some(held);
                    }
                    if activity.draining {
                        break None;
                    }
                    activity = self
                        .writable
                        .wait(activity)
                        .unwrap_or_else(|error| error.into_inner());
                }
            };
            match held {
                Some(held) => self.run_write(held),
                None => return,
            }
        }
    }

    /// Runs one held write on this thread and answers it.
    pub(super) fn run_write(&self, held: Held) {
        let mut admitted = false;
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            if held.traced {
                layerfs_home::trace::steps_begin();
            }
            let outcome = self.dispatch_write(&held, &mut admitted);
            #[cfg(feature = "worker-fault")]
            let outcome = faulted(&self.home, outcome);
            if outcome.as_ref().is_err_and(|error| error.code == "Unknown")
                && !self.keep(&held.request_id, "uncertain", &held.args, &outcome)
            {
                self.lock().unkept.push(Unkept {
                    request_id: held.request_id.clone(),
                    args: held.args.clone(),
                    outcome: outcome.clone(),
                });
            }
            let delivered = super::super::connection::reply(
                &held.stream,
                &held.request_id,
                held.traced,
                outcome.clone(),
            );
            // Only an effect the caller may not know is kept: a Commit
            // whose answer was lost. An uncertain outcome is already kept,
            // and a refusal changed nothing.
            if !delivered && committed(&outcome) {
                let _ = self.keep(&held.request_id, "undelivered", &held.args, &outcome);
            }
            delivered
        }));
        if result.is_err() {
            let _ = layerfs_home::trace::steps_take();
            log("a host write panicked; its answer was lost");
        }
        drop(held);
        let mut activity = self.lock();
        activity.writes.running -= 1;
        activity.writes.held -= 1;
        if admitted {
            activity.admitted -= 1;
            activity.last_call = Instant::now();
        }
        if matches!(result, Ok(true)) {
            activity.answered += 1;
        }
        drop(activity);
        self.notify();
    }

    /// Admits one held write and runs its streamed Store call. Nothing is
    /// resent; a stopped or lost input commits nothing.
    fn dispatch_write(&self, held: &Held, admitted: &mut bool) -> Outcome {
        let input = held.stream.try_clone().map_err(|error| {
            refused("Failed", format!("cannot hold the write's input: {error}"))
        })?;
        let id = {
            let mut activity = self.admit(admitted)?;
            let writes = &mut activity.writes;
            writes.next += 1;
            writes.inputs.push((writes.next, input));
            writes.next
        };
        let mut reader = StreamReader::new(&held.stream);
        let outcome = layerfs_store::host_stream::serve(&self.home, &held.args, &mut reader);
        self.lock().writes.inputs.retain(|(other, _)| *other != id);
        outcome
    }

    /// Writes one outcome's record; on failure the outcome goes to the
    /// worker log, without content, and false is returned.
    fn keep(&self, request_id: &str, kind: &str, args: &SdkValue, outcome: &Outcome) -> bool {
        let Err(error) =
            layerfs_store::host_write::keep(&self.home, request_id, kind, args, outcome)
        else {
            return true;
        };
        let refusal = outcome.as_ref().err();
        log(&format!(
            "could not keep the {kind} outcome of write {request_id} on {}/{} (path {}): {}; outcome {} {}",
            args.get_text("store").unwrap_or_default(),
            args.get_text("project").unwrap_or_default(),
            args.get("fields")
                .and_then(|fields| fields.get_text("path"))
                .unwrap_or_default(),
            error.message,
            refusal.map_or("ok", |refusal| refusal.code.as_str()),
            refusal.map_or(String::new(), |refusal| layerfs_home::json::print(&refusal.detail)),
        ));
        false
    }

    /// Writes the records of held uncertain outcomes, once each; how many
    /// are still held. A drain, an idle exit and a close of their Store
    /// wait for this to reach zero.
    pub(super) fn keep_unkept(&self) -> usize {
        let unkept = std::mem::take(&mut self.lock().unkept);
        let failed: Vec<_> = unkept
            .into_iter()
            .filter(|held| !self.keep(&held.request_id, "uncertain", &held.args, &held.outcome))
            .collect();
        let mut activity = self.lock();
        activity.unkept.extend(failed);
        activity.unkept.len()
    }
}

/// The refusal of a drain or close while uncertain outcomes are held.
pub(super) fn unkept_refusal(count: usize) -> SdkRefusal {
    refused(
        "Unavailable",
        format!(
            "{count} uncertain write outcome(s) could not be written to machine/<host>/retained; the worker holds them and does not drain or close their Store until they are (see its log)"
        ),
    )
}

/// Whether this worker was built with its test fault switches.
#[cfg(feature = "worker-fault")]
pub(super) const FAULTS: bool = true;
/// Whether this worker was built with its test fault switches.
#[cfg(not(feature = "worker-fault"))]
pub(super) const FAULTS: bool = false;

/// Test switch, built only with `worker-fault`: while the home's
/// `machine/<host>/fault-write-unknown` exists, a write's outcome becomes an
/// engine-style `Unknown` that carries the real outcome as its evidence.
#[cfg(feature = "worker-fault")]
fn faulted(home: &std::path::Path, outcome: Outcome) -> Outcome {
    let switch = layerfs_home::home::paths::machine_dir(home, layerfs_home::names::HOST_MACHINE)
        .join("fault-write-unknown");
    if !switch.exists() {
        return outcome;
    }
    let real = match &outcome {
        Ok(answer) => answer.clone(),
        Err(refusal) => SdkValue::text(&refusal.code),
    };
    Err(SdkRefusal {
        code: "Unknown".to_owned(),
        message: "fault: the write's outcome is uncertain".to_owned(),
        detail: SdkValue::map([("fault", SdkValue::Bool(true)), ("answer", real)]),
    })
}

fn committed(outcome: &Outcome) -> bool {
    outcome
        .as_ref()
        .is_ok_and(|answer| answer.get_text("kind") == Some("Committed"))
}
