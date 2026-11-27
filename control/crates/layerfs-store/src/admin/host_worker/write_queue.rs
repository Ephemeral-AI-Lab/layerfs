//! Host worker file writes: their own admission, the streamed Store write,
//! and the records of outcomes their caller may not learn (§8.5).
//!
//! Fairness (§11): a streamed write (`body` content or changes) comes in
//! on the worker's write endpoint, never the ordinary one. Its intake
//! thread reads the request, checks its header there (method, body, field
//! names and scalar values) and holds it for a lane of its own:
//! [`WRITE_THREADS`] write threads. So held writes take no ordinary place
//! or thread, and short calls keep all of them. The write endpoint accepts
//! only while its held writes are below [`HOST_HELD_WRITES`], so further
//! writers wait in its listen queue and are refused only by their own
//! hello bound, before they send anything. A held write waits for a write
//! thread with nothing sent but its request: its caller streams input only
//! after the worker's `go` frame, sent when a write thread starts reading
//! it, so a queued write is never timed out. A waiting write whose caller
//! went away is dropped unrun ([`Writes::purge_gone`]), freeing its place.
//! A write still waiting once the worker drains is refused before any
//! effect. A single-change write (`body` none: edit, mkdir, rm, cp, mv)
//! has no input: it runs on the ordinary thread that read it, or, on a
//! PostgreSQL Store, on the PostgreSQL lane (`postgres.rs`).
//!
//! A PostgreSQL write (#819 P-B) also needs a write slot of its Store's
//! held sessions: a write thread starts the oldest waiting write that can
//! start, taking that slot then, so a PostgreSQL write whose Store or
//! process lends its share of write slots waits on no thread.
//!
//! Drain (§8.3) never stops an admitted write: a running write reads its
//! whole input and finishes as admitted; the drain answer counts the
//! admitted writes still running.
//!
//! Records (§8.5): an engine-uncertain outcome is written to its record
//! before its answer is sent, and a Commit whose answer could not be
//! delivered after it. Each record write is one attempt. One that fails
//! is logged, and an uncertain outcome is then also named `recorded:
//! false` in its own answer and listed in the answer of the drain or stop
//! that ends the worker. Nothing waits on a record: drain, idle exit and a
//! Store's close never do.
use super::server::{log, HostServer};
use crate::backend::postgresql::pool::Reservation;
use layerfs_home::reply::outcome::{refused, Outcome};
use layerfs_machine::route::StreamReader;
#[cfg(doc)]
use layerfs_machine::route::HOST_HELD_WRITES;
use layerfs_protocol::sdk::{SdkRefusal, SdkValue};
use std::collections::VecDeque;
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::Instant;

/// Write threads: streamed writes that run at once.
pub(super) const WRITE_THREADS: usize = 4;
/// Unrecorded uncertain outcomes a drain answer lists; past it only their
/// count grows.
const UNRECORDED_LISTED: usize = 32;

/// One `store.write` request whose input still follows on its connection.
pub(crate) struct Held {
    pub(super) stream: UnixStream,
    pub(super) args: SdkValue,
    pub(super) request_id: String,
    pub(super) traced: bool,
    /// The Store of a PostgreSQL write (#819 P-B), which starts only once
    /// that Store has a free write slot.
    pub(super) postgresql: Option<String>,
}

impl Drop for Held {
    /// A PostgreSQL write's arguments carry its password: overwritten.
    fn drop(&mut self) {
        if self.args.get("password").is_some() {
            crate::admin::host_worker::postgresql::wipe_request(std::mem::take(&mut self.args));
        }
    }
}

/// The held writes of one worker, under its activity lock.
#[derive(Default)]
pub(crate) struct Writes {
    /// Held streamed writes, running or waiting.
    held: usize,
    /// Admitted writes whose Store call has not returned, single-change
    /// writes included.
    running: usize,
    waiting: VecDeque<Held>,
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

    /// The oldest waiting write that can start now, with its PostgreSQL
    /// session slot; draining, the oldest, to be refused unrun.
    fn next(&mut self, home: &Path, draining: bool) -> Option<(Held, Option<Reservation>)> {
        let mut chosen = None;
        for (at, held) in self.waiting.iter().enumerate() {
            match held.postgresql.as_deref() {
                Some(store) if !draining => {
                    if let Some(slot) = crate::backend::postgresql::pool::reserve_write(home, store)
                    {
                        chosen = Some((at, Some(slot)));
                        break;
                    }
                }
                _ => {
                    chosen = Some((at, None));
                    break;
                }
            }
        }
        let (at, slot) = chosen?;
        self.waiting.remove(at).map(|held| (held, slot))
    }

    /// Drops waiting writes whose caller has closed its end: none was sent
    /// `go` and none runs, so each frees its place at once.
    pub(super) fn purge_gone(&mut self) {
        let before = self.waiting.len();
        self.waiting
            .retain(|held| !super::server::peer_gone(&held.stream));
        self.held -= before - self.waiting.len();
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
    /// Holds a streamed write read on the write endpoint. While the worker
    /// drains (its write threads may have ended) it is refused at once,
    /// before any effect: nothing committed. Otherwise it always waits:
    /// the write endpoint's accept limit already bounds held writes.
    pub(super) fn hold_write(&self, held: Held) {
        let mut activity = self.lock();
        if activity.draining {
            drop(activity);
            let mut refusal = refused(
                "Unavailable",
                "the host worker is draining; the write was not run and nothing was committed",
            );
            refusal.detail = SdkValue::map([
                ("request_id", SdkValue::text(&held.request_id)),
                ("state", SdkValue::text("draining")),
            ]);
            super::connection::reply(&held.stream, &held.request_id, false, Err(refusal));
            return;
        }
        activity.writes.held += 1;
        activity.writes.waiting.push_back(held);
        drop(activity);
        self.writable.notify_one();
    }

    /// One write thread: runs held writes until the worker drains and none
    /// waits (a write taken while draining is refused at once, unrun). It
    /// takes the oldest waiting write that can start: a SQLite write, or a
    /// PostgreSQL write whose Store has a free write slot, taken now
    /// (`pool::reserve_write`). A PostgreSQL write whose Store has none
    /// waits, on no thread, while later writes start.
    pub(super) fn work_writes(&self) {
        loop {
            let next = {
                let mut activity = self.lock();
                loop {
                    activity.writes.purge_gone();
                    let draining = activity.draining;
                    if let Some(next) = activity.writes.next(&self.home, draining) {
                        break Some(next);
                    }
                    if draining {
                        break None;
                    }
                    activity = self
                        .writable
                        .wait(activity)
                        .unwrap_or_else(|error| error.into_inner());
                }
            };
            match next {
                Some((held, slot)) => self.run_write(held, slot),
                None => return,
            }
        }
    }

    /// Runs one held write on this write thread, in its PostgreSQL session
    /// `slot` if it has one, and answers it.
    pub(super) fn run_write(&self, held: Held, slot: Option<Reservation>) {
        let mut admitted = false;
        let postgresql = slot.is_some();
        if let Some(slot) = slot {
            crate::backend::postgresql::pool::hold(slot);
        }
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            self.answer_write(&held, &mut admitted)
        }));
        crate::backend::postgresql::pool::release_held();
        if result.is_err() {
            let _ = layerfs_home::log::trace::steps_take();
            log("a host write panicked; its answer was lost");
        }
        drop(held);
        let mut activity = self.lock();
        activity.writes.held -= 1;
        if admitted {
            activity.admitted -= 1;
            activity.last_call = Instant::now();
        }
        if matches!(result, Ok(true)) {
            activity.answered += 1;
        }
        drop(activity);
        if postgresql {
            // its session slot is free for the PostgreSQL lane and writes;
            // only after the activity lock, as in `finish`: a waiter holds
            // it from a failed reserve until it waits, so the wakeup
            // cannot fall between the two
            self.slot_freed();
        }
        self.notify();
    }

    /// Runs one write on this thread and answers it: whether the answer
    /// was delivered. A write thread runs streamed writes; the ordinary
    /// thread that read it runs a single-change write.
    pub(super) fn answer_write(&self, held: &Held, admitted: &mut bool) -> bool {
        if held.traced {
            layerfs_home::log::trace::steps_begin();
        }
        let outcome = self.dispatch_write(held, admitted);
        #[cfg(feature = "worker-fault")]
        let outcome = write_outcome(&self.home, outcome);
        let outcome = match outcome {
            Err(refusal) if refusal.code == "Unknown" => Err(self.record_uncertain(held, refusal)),
            other => other,
        };
        let delivered =
            super::connection::reply(&held.stream, &held.request_id, held.traced, outcome.clone());
        // Only an effect the caller may not know is kept: a Commit whose
        // answer was lost. An uncertain outcome was recorded before its
        // answer, and a refusal changed nothing.
        if !delivered && committed(&outcome) {
            let _ = self.keep(&held.request_id, "undelivered", &held.args, &outcome);
        }
        delivered
    }

    /// Records an uncertain outcome before its answer, once. When that
    /// fails, the answer says so (`recorded: false`) and the outcome is
    /// listed for the drain answer.
    fn record_uncertain(&self, held: &Held, mut refusal: SdkRefusal) -> SdkRefusal {
        let outcome = Err(refusal.clone());
        let recorded = self.keep(&held.request_id, "uncertain", &held.args, &outcome);
        refusal.detail =
            layerfs_home::disk::with_field(refusal.detail, "recorded", SdkValue::Bool(recorded));
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

    /// Admits one write, counted running until its Store call returns,
    /// tells its caller to send its input (`go`, for a streamed body), and
    /// runs its Store call. Nothing is resent; a lost input commits nothing.
    fn dispatch_write(&self, held: &Held, admitted: &mut bool) -> Outcome {
        self.admit(admitted)?.writes.running += 1;
        let _running = Running(self);
        // a single-change write has no input to ask for
        let streamed = matches!(held.args.get_text("body"), Some("content" | "changes"));
        if streamed && !super::connection::go(&held.stream, &held.request_id) {
            return Err(refused(
                "Unavailable",
                "the write's caller was gone before its input was sent; nothing was committed",
            ));
        }
        let mut reader = StreamReader::new(&held.stream);
        crate::admin::host_worker::stream::serve(&self.home, &held.args, &mut reader)
    }

    /// Writes one outcome's record, one attempt; on failure the outcome
    /// goes to the worker log, without content, and false is returned.
    fn keep(&self, request_id: &str, kind: &str, args: &SdkValue, outcome: &Outcome) -> bool {
        let Err(error) =
            crate::admin::host_worker::retained::keep(&self.home, request_id, kind, args, outcome)
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

/// An admitted write's place in [`Writes::running`], given back however
/// its call ends, a panic included.
struct Running<'a>(&'a HostServer);

impl Drop for Running<'_> {
    fn drop(&mut self) {
        self.0.lock().writes.running -= 1;
    }
}

fn committed(outcome: &Outcome) -> bool {
    outcome
        .as_ref()
        .is_ok_and(|answer| answer.get_text("kind") == Some("Committed"))
}

/// Host worker write switch (S2.2): while the home's
/// `machine/<host>/fault-write-unknown` exists, a write's outcome becomes
/// an engine-style `Unknown` that carries the real outcome as evidence.
#[cfg(feature = "worker-fault")]
fn write_outcome(
    home: &std::path::Path,
    outcome: layerfs_home::reply::outcome::Outcome,
) -> layerfs_home::reply::outcome::Outcome {
    let switch = layerfs_home::disk::paths::machine_dir(home, layerfs_home::names::HOST_MACHINE)
        .join("fault-write-unknown");
    if !switch.exists() {
        return outcome;
    }
    let real = match &outcome {
        Ok(answer) => answer.clone(),
        Err(refusal) => layerfs_protocol::sdk::SdkValue::text(&refusal.code),
    };
    Err(layerfs_protocol::sdk::SdkRefusal {
        code: "Unknown".to_owned(),
        message: "fault: the write's outcome is uncertain".to_owned(),
        detail: layerfs_protocol::sdk::SdkValue::map([
            ("fault", layerfs_protocol::sdk::SdkValue::Bool(true)),
            ("answer", real),
        ]),
    })
}
