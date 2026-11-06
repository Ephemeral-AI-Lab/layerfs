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
//! dispatches and holds the steps the calls share; each call is in its own
//! file (`list`, `status`, `create`, `remove`, `transfer/phases` for push
//! and pull).
use super::create::create;
use super::list::list;
use super::remove::remove;
use super::status::status;
use super::transfer::phases::{pull, push};
use crate::common::context::Context;
use crate::common::lease;
use crate::common::place::Executor;
use crate::store::record::{self, StoreDerived, StoreRecord, StoreState};
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_home::home::{self, paths};
use layerfs_home::op::{self, Op};
use layerfs_home::outcome::{failed, refused, Outcome, View};
use std::path::Path;

type Result<T> = std::result::Result<T, SdkRefusal>;

pub(super) fn record_of(home: &Path, name: &str) -> Result<StoreRecord> {
    StoreRecord::of(home, name)
}

/// A Store as callers see it, with its projects (and a list item's
/// `state`): never the cursor key.
pub(super) fn public(
    home: &Path,
    record: &StoreRecord,
    view: View,
    state: Option<StoreState>,
) -> Outcome {
    let projects = paths::names(&paths::store_dir(home, record.name()).join("project"));
    let derived = StoreDerived {
        projects: projects.map_err(failed)?,
        state,
    };
    record.to_sdk(view, &derived)
}

/// Runs one sqlite `store` call.
pub fn run(context: &mut Context<'_>, call: &str, args: &SdkValue) -> Outcome {
    let home = context.home.clone();
    let name = args.get_text("name").unwrap_or("");
    match call {
        "list" => list(&home),
        "get" => public(&home, &record_of(&home, name)?, View::Full, None),
        "status" => status(&home, name),
        "create" => create(&home, name),
        "remove" => remove(&home, name),
        "push" => push(context, name, args.get_text("machine").unwrap_or("")),
        "pull" => pull(context, name),
        _ => Err(refused("InvalidInput", format!("no call store.{call}"))),
    }
}

/// One step on the machine that holds a Store's file.
pub(super) fn step(home: &Path, executor: &mut Executor, op: &str, args: SdkValue) -> Outcome {
    match executor {
        Executor::Local => crate::store::transfer::steps::run(home, op, &args),
        Executor::Worker(worker) => {
            let request = SdkValue::map([("op", SdkValue::text(op)), ("args", args)]);
            worker.call("store.file", request, &mut |_| {})
        }
    }
}

pub(super) fn fields_map(fields: Vec<(&str, SdkValue)>) -> SdkValue {
    SdkValue::Map(fields.into_iter().map(|(n, v)| (n.to_owned(), v)).collect())
}

pub(super) fn of_store(name: &str, more: Vec<(&str, SdkValue)>) -> SdkValue {
    let mut fields = vec![("store", SdkValue::text(name))];
    fields.extend(more);
    fields_map(fields)
}

/// Starts a push, pull or remove: under the home lock (the Sandbox check) and
/// the Store's lock, re-reads the record, lets `check` refuse it or answer
/// what the call needs, finds the Store idle and writes `op` into the
/// record, which refuses every later lease. Answers `check`'s answer.
pub(super) fn begin<T>(
    home: &Path,
    name: &str,
    op: &Op,
    check: impl FnOnce(&StoreRecord) -> Result<T>,
) -> Result<T> {
    let _lock = home::lock::take(home)?;
    let _store = record::lock(home, name)?;
    let record = record_of(home, name)?;
    let answer = check(&record)?;
    crate::inventory::store_idle(home, name)?;
    record.with(op::FIELD, op.to_record().clone()).write(home)?;
    Ok(answer)
}

/// After `op` is written: closes this process's own open copy (Store I/O,
/// so past the locks; `InUse` while a call runs on it), then, under the
/// home lock and the Store's lock, finds the Store idle once more: no other
/// holder, no lease of this process, no Sandbox. Answers this process's
/// lease, taken there.
pub(super) fn settle(home: &Path, name: &str) -> Result<lease::Lease> {
    crate::store::open::close(&paths::store_file(home, name))?;
    let _lock = home::lock::take(home)?;
    let _store = record::lock(home, name)?;
    if lease::held_here(home, name) {
        return Err(refused(
            "InUse",
            format!("Store {name} is in use in this process"),
        ));
    }
    crate::inventory::store_idle(home, name)?;
    lease::hold_locked(home, name)
}

/// Under the Store's lock, replaces the record's `op` with `op` (`None`
/// drops it) while it is still the operation `id`: an operation never
/// overwrites another's intent.
pub(super) fn replace_op(home: &Path, name: &str, id: &str, op: Option<&Op>) -> Result<()> {
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
pub(super) fn advance(home: &Path, name: &str, op: &mut Op, phase: &str) -> Result<()> {
    *op = op.clone().at(phase);
    replace_op(home, name, op.id(), Some(op))
}

/// Ends a push, pull or remove that failed at `phase` after writing `op`: a
/// certain failure drops it (or puts back `previous`, the unresolved intent
/// a remove would have cleared); an uncertain one keeps it, marked
/// uncertain, and is logged to `log/uncertain.jsonl`.
pub(super) fn ended(
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
pub(crate) fn progress(context: &mut Context<'_>, phase: &str, bytes: Option<(u64, u64)>) {
    let count = |n: u64| SdkValue::Int(n as i64);
    let (done, total) = match bytes {
        Some((done, total)) => (count(done), count(total)),
        None => (SdkValue::Null, SdkValue::Null),
    };
    context.progress(SdkValue::map([
        ("phase", SdkValue::text(phase)),
        ("done", done),
        ("total", total),
    ]));
}
