//! Host worker file writes: their own admission, the streamed Store write,
//! and the records of outcomes their caller may not learn (§8.5).
//!
//! Fairness (§11): a `store.write` request, once read, leaves its ordinary
//! connection slot and thread and is held apart, at most [`WRITES_HELD`]
//! at once, for a lane of its own: [`WRITE_THREADS`] write threads. A
//! write never runs on an ordinary thread, so however many writes stream,
//! short calls keep every ordinary thread and slot. A held write waits for
//! a write thread while its input fits the socket buffer; its caller's
//! own send bound ends a wait that never gets one. Past [`WRITES_HELD`],
//! or once the worker drains, a write is refused before any effect.
//!
//! Drain (§8.3) stops the input of every running write: a write whose
//! `end` frame was not read yet is then refused with nothing committed (its
//! input was never complete), and one past `end` finishes as admitted.
//!
//! Records (§8.5): an engine-uncertain outcome is written to its record
//! before its answer is sent, and a Commit whose answer could not be
//! delivered after it. Each record write is one attempt. One that fails
//! is logged, and an uncertain outcome is then also named `recorded:
//! false` in its own answer and listed in the answer of the drain or stop
//! that ends the worker. Nothing waits on a record: drain, idle exit and a
//! Store's close never do.
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
/// Unrecorded uncertain outcomes a drain answer lists; past it only their
/// count grows.
const UNRECORDED_LISTED: usize = 32;

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
    /// Uncertain outcomes whose record could not be written, for the
    /// drain answer: the first [`UNRECORDED_LISTED`], and how many.
    unrecorded: Vec<SdkValue>,
    unrecorded_count: usize,
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

    /// How many uncertain outcomes have no record.
    pub(super) fn unrecorded_count(&self) -> usize {
        self.unrecorded_count
    }

    /// The drain answer's account of uncertain outcomes with no record.
    pub(super) fn unrecorded(&self) -> [(&'static str, SdkValue); 2] {
        [
            ("unrecorded", SdkValue::List(self.unrecorded.clone())),
            (
                "unrecorded_count",
                SdkValue::Int(self.unrecorded_count as i64),
            ),
        ]
    }
}

impl HostServer {
    /// Holds a write read on an ordinary connection. Past [`WRITES_HELD`],
    /// or while the worker drains (its write threads may have ended), it is
    /// refused at once, before any effect: nothing committed.
    pub(in crate::worker_server) fn hold_write(&self, held: Held) {
        let mut activity = self.lock();
        let refusal = if activity.draining {
            Some((
                "the host worker is draining; the write was not run and nothing was committed"
                    .to_owned(),
                "draining",
            ))
        } else if activity.writes.held >= WRITES_HELD {
            Some((
                format!(
                    "the host worker already holds {WRITES_HELD} streamed writes; the write was not run and nothing was committed"
                ),
                "writes-full",
            ))
        } else {
            None
        };
        if let Some((message, state)) = refusal {
            drop(activity);
            let mut refusal = refused("Unavailable", message);
            refusal.detail = SdkValue::map([
                ("request_id", SdkValue::text(&held.request_id)),
                ("state", SdkValue::text(state)),
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
            let outcome = super::super::fault::write_outcome(&self.home, outcome);
            let outcome = match outcome {
                Err(refusal) if refusal.code == "Unknown" => {
                    Err(self.record_uncertain(&held, refusal))
                }
                other => other,
            };
            let delivered = super::super::connection::reply(
                &held.stream,
                &held.request_id,
                held.traced,
                outcome.clone(),
            );
            // Only an effect the caller may not know is kept: a Commit
            // whose answer was lost. An uncertain outcome was recorded
            // before its answer, and a refusal changed nothing.
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

    /// Records an uncertain outcome before its answer, once. When that
    /// fails, the answer says so (`recorded: false`) and the outcome is
    /// listed for the drain answer.
    fn record_uncertain(&self, held: &Held, mut refusal: SdkRefusal) -> SdkRefusal {
        let outcome = Err(refusal.clone());
        let recorded = self.keep(&held.request_id, "uncertain", &held.args, &outcome);
        refusal.detail =
            layerfs_home::home::with_field(refusal.detail, "recorded", SdkValue::Bool(recorded));
        if !recorded {
            let fields = held.args.get("fields");
            let mut activity = self.lock();
            let writes = &mut activity.writes;
            writes.unrecorded_count += 1;
            if writes.unrecorded.len() < UNRECORDED_LISTED {
                writes.unrecorded.push(SdkValue::map([
                    ("request_id", SdkValue::text(&held.request_id)),
                    ("store", SdkValue::opt_text(held.args.get_text("store"))),
                    ("project", SdkValue::opt_text(held.args.get_text("project"))),
                    (
                        "branch",
                        SdkValue::opt_text(fields.and_then(|f| f.get_text("branch"))),
                    ),
                    (
                        "path",
                        SdkValue::opt_text(fields.and_then(|f| f.get_text("path"))),
                    ),
                    ("detail", refusal.detail.clone()),
                ]));
            }
        }
        refusal
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

    /// Writes one outcome's record, one attempt; on failure the outcome
    /// goes to the worker log, without content, and false is returned.
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
}

fn committed(outcome: &Outcome) -> bool {
    outcome
        .as_ref()
        .is_ok_and(|answer| answer.get_text("kind") == Some("Committed"))
}
