//! The sqlite half of the `store` family (#533 step 8, H §7.3; call surface
//! 2.0): create, remove and move. A move and a remove read leases under
//! the home lock and the Store's lock (#536 §8), refuse `InUse` and write
//! the record's `op` (#536 §7.1), which refuses every later lease and
//! operation; then, past the locks, they close this process's own open copy
//! and find the Store idle once more before any step. The locks are held
//! only for those checks and record writes: never across a machine start, a
//! Store step or Store I/O. A move runs its phases once each (check,
//! snapshot, send, verify, install, record, retire), each written to `op`
//! as it starts; a failure names its phase and keeps whatever exists. An
//! uncertain step (`Unknown`, or any failure of the record phase, after
//! both copies exist) keeps both copies, leaves `op` marked uncertain and
//! is logged to `log/uncertain.jsonl`; nothing is cleaned up on a guess.
//! A phase a crash would leave unknown (a move's install and record, a
//! remove's remove, a create's create on a machine) is written uncertain
//! before it starts; a new move first deletes the copy such an ended one
//! left. This file holds each call's entry and the steps the calls share;
//! each call's body is in its own file (`create`, `remove`,
//! `transfer/phases` for a move). `store.list` and `store.get` are the
//! kind-neutral root's (`list`, `get`).
use crate::intent;
use crate::lease;
use crate::place::Executor;
use crate::record::{self, StoreRecord};
use crate::sqlite::transfer::step::{StepFrame, StoreStep};
use crate::Related;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_home::events::Events;
use layerfs_home::home::{self, paths};
use layerfs_home::op::{self, Op};
use layerfs_home::outcome::{refused, Outcome};
use std::path::Path;

type Result<T> = std::result::Result<T, SdkRefusal>;

/// One step on the machine that holds a Store's file: in this process, or
/// its worker's `store.file` frame.
pub fn step(home: &Path, executor: &mut Executor, frame: &StepFrame) -> Outcome {
    match executor {
        Executor::Local => crate::sqlite::transfer::steps::run(home, frame),
        Executor::Worker(worker) => worker.call("store.file", frame.encode(), &mut |_| {}),
    }
}

/// [`step`] `step` on Store `name`.
pub fn step_on(home: &Path, executor: &mut Executor, name: &str, step: StoreStep) -> Outcome {
    self::step(home, executor, &StepFrame::on(name, step))
}

/// [`step`], passing on the events a worker sends and reporting a local
/// step's own byte progress (the hashing of a `snapshot` or `verify`) as
/// phase `phase` (#536 §10b). A worker's answer that sent no final count
/// still ends at its whole `bytes`.
pub fn step_observed(
    home: &Path,
    events: &mut Events<'_>,
    phase: &str,
    executor: &mut Executor,
    frame: &StepFrame,
) -> Outcome {
    match executor {
        Executor::Local => {
            crate::sqlite::transfer::steps::run_observed(home, frame, &mut |done, total| {
                progress(events, phase, Some((done, total)))
            })
        }
        Executor::Worker(worker) => {
            let mut saw_final = false;
            let result = worker.call("store.file", frame.encode(), &mut |event| {
                if let Some(p) = event.get("progress") {
                    let (done, total) = (p.get_int("done"), p.get_int("total"));
                    if done.is_some() && done == total {
                        saw_final = true;
                    }
                }
                events.event(event);
            })?;
            if matches!(frame.step.name(), "snapshot" | "verify") && !saw_final {
                if let Some(bytes) = result.get_int("bytes").filter(|&b| b >= 0) {
                    let total = bytes as u64;
                    progress(events, phase, Some((total, total)));
                }
            }
            Ok(result)
        }
    }
}

/// Starts a move or a remove: under the home lock (the Sandbox check) and
/// the Store's lock, re-reads the record, lets `check` refuse it or answer
/// what the call needs, finds the Store idle and writes `op` into the
/// record, which refuses every later lease. Answers `check`'s answer.
pub fn begin<T>(
    home: &Path,
    name: &str,
    op: &Op,
    related: Related,
    check: impl FnOnce(&StoreRecord) -> Result<T>,
) -> Result<T> {
    let _lock = home::lock::take(home)?;
    let _store = record::lock(home, name)?;
    let record = StoreRecord::of(home, name)?;
    let answer = check(&record)?;
    (related.store_idle)(home, name)?;
    record.with(op::FIELD, op.to_record().clone()).write(home)?;
    Ok(answer)
}

/// After `op` is written: closes this process's own open copy (Store I/O,
/// so past the locks; `InUse` while a call runs on it), then, under the
/// home lock and the Store's lock, finds the Store idle once more: no other
/// holder, no lease of this process, no Sandbox. Answers this process's
/// lease, taken there.
pub fn settle(home: &Path, name: &str, related: Related) -> Result<lease::Lease> {
    crate::open::close(&paths::store_file(home, name))?;
    let _lock = home::lock::take(home)?;
    let _store = record::lock(home, name)?;
    if lease::held_here(home, name) {
        return Err(refused(
            "InUse",
            format!("Store {name} is in use in this process"),
        ));
    }
    (related.store_idle)(home, name)?;
    lease::hold_locked(home, name)
}

/// Under the Store's lock, replaces the record's `op` with `op` (`None`
/// drops it) while it is still the operation `id` ([`intent::replace`]).
pub fn replace_op(home: &Path, name: &str, id: &str, op: Option<&Op>) -> Result<()> {
    intent::replace(home, name, id, op)
}

/// `op` moved to `phase`, in the record too.
pub fn advance(home: &Path, name: &str, op: &mut Op, phase: &str) -> Result<()> {
    *op = op.clone().at(phase);
    replace_op(home, name, op.id(), Some(op))
}

/// Ends a move, a remove or a create that failed at `phase` after writing `op`: a
/// certain failure drops it (or puts back `previous`, the unresolved intent
/// a remove would have cleared); an uncertain one keeps it, marked
/// uncertain, and is logged to `log/uncertain.jsonl`.
pub fn ended(
    home: &Path,
    name: &str,
    op: &Op,
    phase: &str,
    refusal: &SdkRefusal,
    uncertain: bool,
    previous: Option<&Op>,
) {
    if uncertain {
        let unknown = op.clone().at(phase).as_uncertain();
        let _ = replace_op(home, name, op.id(), Some(&unknown));
        unknown.log_uncertain(home, "store", name, &refusal.message);
    } else {
        let _ = replace_op(home, name, op.id(), previous);
    }
}

/// `{ phase, done, total }` (#536 §10b): `done` and `total` are bytes
/// while bytes are sent (`bytes`), and null in every other phase.
pub fn progress(events: &mut Events<'_>, phase: &str, bytes: Option<(u64, u64)>) {
    let count = |n: u64| SdkValue::Int(n as i64);
    let (done, total) = match bytes {
        Some((done, total)) => (count(done), count(total)),
        None => (SdkValue::Null, SdkValue::Null),
    };
    events.progress(SdkValue::map([
        ("phase", SdkValue::text(phase)),
        ("done", done),
        ("total", total),
    ]));
}
