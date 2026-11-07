//! The sqlite half of the `store` family (#533 step 8, H §7.3): list, get,
//! status, create, remove, push and pull. A push, a pull and a remove read leases under
//! the home lock and the Store's lock (#536 §8), refuse `InUse` and write
//! the record's `op` (#536 §7.1), which refuses every later lease and
//! operation; then, past the locks, they close this process's own open copy
//! and find the Store idle once more before any step. The locks are held
//! only for those checks and record writes: never across a machine start, a
//! Store step or Store I/O. A push or pull runs its phases once each (check,
//! snapshot, send, verify, install, record, retire), each written to `op`
//! as it starts; a failure names its phase and keeps whatever exists. An
//! uncertain step (`Unknown`, or any failure of the record phase, after
//! both copies exist) keeps both copies, leaves `op` marked uncertain and
//! is logged to `log/uncertain.jsonl`; nothing is cleaned up on a guess.
//! A phase a crash would leave unknown (a push's or pull's install and
//! record, a remove's remove) is written uncertain before it starts; a new
//! push or pull first deletes the copy such an ended one left. This file
//! holds each call's entry and the steps the calls share; each call's body
//! is in its own file (`list`, `status`, `create`, `remove`, `transfer/phases` for push
//! and pull).
use crate::lease;
use crate::place::Executor;
use crate::record::{self, StoreDerived, StoreRecord, StoreState};
use crate::sqlite::transfer::step::{StepFrame, StoreStep};
use crate::Related;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_home::events::Events;
use layerfs_home::home::{self, paths};
use layerfs_home::names::StoreName;
use layerfs_home::op::{self, Op};
use layerfs_home::outcome::{failed, refused, Outcome, View};
use std::path::Path;

type Result<T> = std::result::Result<T, SdkRefusal>;

pub fn record_of(home: &Path, name: &str) -> Result<StoreRecord> {
    StoreRecord::of(home, name)
}

/// A Store as callers see it, with its projects (and a list item's
/// `state`): never the cursor key.
pub fn public(home: &Path, record: &StoreRecord, view: View, state: Option<StoreState>) -> Outcome {
    let projects = paths::names(&paths::store_dir(home, record.name()).join("project"));
    let derived = StoreDerived {
        projects: projects.map_err(failed)?,
        state,
    };
    record.to_sdk(view, &derived)
}

/// `store.list`: every Store, from the home's records.
pub fn list(home: &Path) -> Outcome {
    crate::list::list(home)
}

/// `store.get`: a Store's record.
pub fn get(home: &Path, name: &StoreName) -> Outcome {
    public(home, &record_of(home, name)?, View::Full, None)
}

/// `store.status`: a Store's state.
pub fn status(home: &Path, name: &StoreName) -> Outcome {
    crate::status::status(home, name)
}

/// `store.create`: an empty sqlite Store on native.
pub fn create(home: &Path, name: &StoreName) -> Outcome {
    crate::sqlite::create::create(home, name)
}

/// `store.remove`: a Store no Sandbox uses, with its file.
pub fn remove(home: &Path, name: &StoreName, related: Related) -> Outcome {
    crate::sqlite::remove::remove(home, name, related)
}

/// `store.push(name, machine)`. The names are checked in its check phase.
#[derive(Clone, Debug, Default)]
pub struct StorePush {
    /// The Store.
    pub name: String,
    /// The machine it goes to.
    pub machine: String,
}

/// `store.push`: a sqlite Store from native to a machine.
pub fn push(
    home: &Path,
    events: &mut Events<'_>,
    request: &StorePush,
    related: Related,
) -> Outcome {
    crate::sqlite::transfer::phases::push(home, events, &request.name, &request.machine, related)
}

/// `store.pull`: a sqlite Store from its machine to native. The name is
/// checked in its check phase.
pub fn pull(home: &Path, events: &mut Events<'_>, name: &str, related: Related) -> Outcome {
    crate::sqlite::transfer::phases::pull(home, events, name, related)
}

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

pub fn fields_map(fields: Vec<(&str, SdkValue)>) -> SdkValue {
    SdkValue::Map(fields.into_iter().map(|(n, v)| (n.to_owned(), v)).collect())
}

/// Starts a push, pull or remove: under the home lock (the Sandbox check) and
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
    let record = record_of(home, name)?;
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
/// drops it) while it is still the operation `id`: an operation never
/// overwrites another's intent.
pub fn replace_op(home: &Path, name: &str, id: &str, op: Option<&Op>) -> Result<()> {
    let _store = record::lock(home, name)?;
    let record = record_of(home, name)?;
    if record.op().is_none_or(|held| held.id() != id) {
        return Ok(());
    }
    match op {
        Some(op) => record.with(op::FIELD, op.to_record().clone()),
        None => record.without(op::FIELD),
    }
    .write(home)
}

/// `op` moved to `phase`, in the record too.
pub fn advance(home: &Path, name: &str, op: &mut Op, phase: &str) -> Result<()> {
    *op = op.clone().at(phase);
    replace_op(home, name, op.id(), Some(op))
}

/// Ends a push, pull or remove that failed at `phase` after writing `op`: a
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
