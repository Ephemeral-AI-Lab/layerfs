//! The `project.stage` parts of one folder copy on their way to a
//! machine's worker (#536 H11a): each part is sent on its own connection
//! from its own thread, up to [`AT_ONCE`] parts held at once, answered in
//! any order. The worker serves each connection on its own thread and
//! applies a part's data records in any order ([`super::staged`]).
//!
//! One attempt per part: a part is sent once, never again. After the first
//! failure (a part refused, not reached or whose answer was lost, or this
//! process's own walk refusing), no new part is sent; the parts in flight
//! are waited for, and that first failure is the copy's. A certain failure
//! discards the copy with one more request, unless a part's answer was
//! lost: that part may still be written, so the copy is left and named.
use super::frame::{StageFrame, StageStep};
use super::send::{left, Copy};
use crate::place::Worker;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_home::events::Events;
use layerfs_home::outcome::{refused, Outcome};
use layerfs_home::trace;
use layerfs_machine::worker::Unreached;
use std::sync::mpsc;
use std::time::Duration;

type Result<T> = std::result::Result<T, SdkRefusal>;
type Steps = Vec<(String, Duration)>;

/// The most parts held at once, in flight or being filled (4 x 8 MiB):
/// while three are in flight the next is read and framed here, so this
/// process's reading, the transfer to the machine and the worker's writes
/// overlap. More connections would share the same port forward and disk.
pub const AT_ONCE: usize = 4;

/// One part's end, from the thread that sent it.
struct Answer {
    /// The file bytes the part carried.
    bytes: u64,
    outcome: std::result::Result<Outcome, Unreached>,
    /// The steps its thread timed, while the copy's are timed.
    steps: Steps,
}

/// Sends its answer when dropped, so every part answers exactly once: a
/// thread that never ran sent nothing (`Unreached`); one that ended without
/// an outcome (a panic) may have sent its part (`Unknown`).
struct Reply {
    to: mpsc::Sender<Answer>,
    bytes: u64,
    answer: Option<(std::result::Result<Outcome, Unreached>, Steps)>,
}

impl Drop for Reply {
    fn drop(&mut self) {
        let (outcome, steps) = self.answer.take().unwrap_or_else(|| {
            let why = "project.stage: a part's thread ended before its answer; it may have been sent, and was not resent";
            (Ok(Err(refused("Unknown", why))), Vec::new())
        });
        let bytes = self.bytes;
        let _ = self.to.send(Answer {
            bytes,
            outcome,
            steps,
        });
    }
}

/// The parts of one copy sent so far.
pub struct Parts {
    worker: Worker,
    store: String,
    name: String,
    machine: String,
    id: String,
    to: mpsc::Sender<Answer>,
    answers: mpsc::Receiver<Answer>,
    in_flight: usize,
    /// File bytes the worker has written, of `total`.
    sent: u64,
    total: u64,
    /// Whether a part reached the worker.
    asked: bool,
    /// Whether a part's answer was lost.
    uncertain: bool,
    /// The first failure.
    failed: Option<SdkRefusal>,
}

impl Parts {
    /// No part sent yet of `copy` (`total` file bytes) to `worker`.
    pub fn new(worker: &Worker, copy: &Copy<'_>, total: u64) -> Self {
        let (to, answers) = mpsc::channel();
        Self {
            worker: worker.clone(),
            store: copy.store.to_owned(),
            name: copy.name.to_owned(),
            machine: copy.machine.to_owned(),
            id: copy.id.clone(),
            to,
            answers,
            in_flight: 0,
            sent: 0,
            total,
            asked: false,
            uncertain: false,
            failed: None,
        }
    }

    fn request(&self, step: StageStep) -> SdkValue {
        StageFrame {
            store: self.store.clone(),
            name: self.name.clone(),
            id: self.id.clone(),
            step,
        }
        .encode()
    }

    /// The first failure, if there is one: nothing more is sent after it.
    pub fn check(&self) -> Result<()> {
        self.failed.clone().map_or(Ok(()), Err)
    }

    /// Sends `part` (holding `bytes` of file data) once, on a new
    /// connection from a new thread, then waits while [`AT_ONCE`] parts are
    /// in flight. The first failure seen is answered, and nothing is sent
    /// once there is one.
    pub fn send(&mut self, events: &mut Events<'_>, part: Vec<u8>, bytes: u64) -> Result<()> {
        self.check()?;
        // framed here: the request carries this thread's home id and step
        // timing; the part's buffer is freed once it is framed
        let op = "project.stage";
        let request = layerfs_machine::worker::request(op, self.request(StageStep::Write(part)));
        let request = match request {
            Ok(request) => request,
            Err(refusal) => return Err(self.failed.get_or_insert(refusal).clone()),
        };
        let stepping = trace::stepping();
        let mut worker = self.worker.clone();
        let mut reply = Reply {
            to: self.to.clone(),
            bytes,
            answer: Some((Err(Unreached(not_started())), Vec::new())),
        };
        // a thread that does not start drops its reply: not sent
        self.in_flight += 1;
        let _ = std::thread::Builder::new()
            .name("layerfs-stage".to_owned())
            .spawn(move || {
                reply.answer = None;
                if stepping {
                    trace::steps_begin();
                }
                let outcome = worker.attempt_framed(op, &request, &mut |_| {});
                let steps = if stepping {
                    trace::steps_take()
                } else {
                    Vec::new()
                };
                reply.answer = Some((outcome, steps));
            });
        while let Ok(answer) = self.answers.try_recv() {
            self.record(events, answer);
        }
        while self.in_flight >= AT_ONCE {
            self.wait_one(events);
        }
        self.check()
    }

    /// Waits for every part in flight.
    pub fn settle(&mut self, events: &mut Events<'_>) {
        while self.in_flight > 0 {
            self.wait_one(events);
        }
    }

    fn wait_one(&mut self, events: &mut Events<'_>) {
        // every part answers once (`Reply`), and this holds a sender, so
        // this waits for the next answer only
        if let Ok(answer) = self.answers.recv() {
            self.record(events, answer);
        }
    }

    fn record(&mut self, events: &mut Events<'_>, answer: Answer) {
        self.in_flight -= 1;
        for (name, took) in answer.steps {
            trace::step_of(name, took);
        }
        match answer.outcome {
            Ok(Ok(_)) => {
                self.asked = true;
                self.sent += answer.bytes;
                let done = Some((self.sent, self.total));
                crate::sqlite::family::progress(events, "send", done);
            }
            Ok(Err(refusal)) => {
                self.asked = true;
                self.uncertain |= refusal.code == "Unknown";
                self.failed.get_or_insert(refusal);
            }
            Err(Unreached(refusal)) => {
                self.failed.get_or_insert(refusal);
            }
        }
    }

    /// Ends the copy once `walked` (the walk and its last parts) ended:
    /// waits for every part in flight, then answers the first failure, if
    /// any. A certain failure discards the copy (one request); a lost
    /// answer leaves it, named.
    pub fn end(mut self, events: &mut Events<'_>, walked: Result<()>) -> Result<()> {
        if let Err(refusal) = walked {
            self.failed.get_or_insert(refusal);
        }
        self.settle(events);
        // the copy's parts were sent several at once: one step for them all
        trace::step("send");
        let Some(mut refusal) = self.failed.take() else {
            return Ok(());
        };
        let left = left(&self.machine, &self.id);
        if refusal.code == "Unknown" {
            refusal.message = format!("{}; its copy may remain at {left}", refusal.message);
            return Err(refusal);
        }
        if self.uncertain {
            refusal.message = format!(
                "{}; another part's answer was lost, so its copy was not discarded and may remain at {left}",
                refusal.message
            );
            return Err(refusal);
        }
        if !self.asked {
            return Err(refusal);
        }
        let discard = self.request(StageStep::Discard);
        if let Err(why) = self.worker.call("project.stage", discard, &mut |_| {}) {
            refusal.message = format!(
                "{}; its copy was not discarded ({}) and may remain at {left}",
                refusal.message, why.message
            );
        }
        Err(refusal)
    }
}

fn not_started() -> SdkRefusal {
    refused(
        "Failed",
        "project.stage: no thread could be started to send a part; it was not sent",
    )
}
