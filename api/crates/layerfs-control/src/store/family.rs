//! The sqlite half of the `store` family (#533 step 8, H §7.3): list, get,
//! status, create, remove and move. `move` and `remove` read leases under
//! the home lock, refuse `InUse` and write the record's `in_progress`, which
//! refuses every later lease; then, past the lock, they close this process's
//! own open copy and find the Store idle once more before any step. The home
//! lock is held only for those checks and record writes: never across a
//! machine start, a Store step or Store I/O. A move runs its phases once
//! each (check, snapshot, send, verify, install, record, retire); a failure
//! names its phase and keeps whatever exists. An uncertain step (`Unknown`,
//! or any failure of the record phase, after both copies exist) keeps both
//! copies, leaves the record's `in_progress` and is logged to
//! `log/uncertain.jsonl`; nothing is cleaned up on a guess. This file
//! dispatches and holds the steps the calls share; each call is in its own
//! file (`list`, `status`, `create`, `remove`, `transfer/phases` for move).
use super::create::create;
use super::list::list;
use super::remove::remove;
use super::status::status;
use super::transfer::phases::move_store;
use crate::common::context::Context;
use crate::common::home::lease;
use crate::common::home::{self, paths};
use crate::common::outcome::{failed, refused, Outcome, View};
use crate::common::place::Executor;
use crate::store::record::{StoreDerived, StoreRecord, StoreState};
use layerfs_bridge::control::{SdkRefusal, SdkValue};
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
        "move" => {
            let to = args.get_text("machine").unwrap_or("");
            move_store(context, name, to)
        }
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

/// Refuses `InUse` while another process holds the Store, a Sandbox of
/// one of its projects runs, or a live process moves or removes it. Under
/// the home lock.
pub(super) fn idle(home: &Path, name: &str, record: &StoreRecord) -> Result<()> {
    record.moving()?;
    let holders = lease::holders(home, name)?;
    if !holders.is_empty() {
        return Err(refused(
            "InUse",
            format!("Store {name} is open in process {holders:?}"),
        ));
    }
    crate::inventory::can_retire_store(home, name)?;
    Ok(())
}

/// After `in_progress` is written: closes this process's own open copy
/// (Store I/O, so past the home lock; `InUse` while a call runs on it),
/// then, under the home lock, finds the Store idle once more: no other
/// holder, no lease of this process, no Sandbox. Answers this process's
/// lease, taken there.
pub(super) fn settle(home: &Path, name: &str) -> Result<lease::Lease> {
    crate::store::open::close(&paths::store_file(home, name))?;
    let _lock = home::lock::take(home)?;
    if lease::held_here(home, name) {
        return Err(refused(
            "InUse",
            format!("Store {name} is in use in this process"),
        ));
    }
    let holders = lease::holders(home, name)?;
    if !holders.is_empty() {
        return Err(refused(
            "InUse",
            format!("Store {name} is open in process {holders:?}"),
        ));
    }
    crate::inventory::can_retire_store(home, name)?;
    lease::hold_locked(home, name)
}

/// The record's `in_progress` for this process's `op`, with `more`.
pub(super) fn in_progress(op: &str, more: Vec<(&str, SdkValue)>) -> SdkValue {
    let mut fields = vec![("op", SdkValue::text(op))];
    fields.extend(more);
    fields.extend(lease::identity());
    fields.push(("started", SdkValue::Int(home::now_ms())));
    fields_map(fields)
}

/// Ends a move or remove that failed after writing `in_progress`: a certain
/// failure clears it; an uncertain one keeps it and is logged.
pub(super) fn ended(
    home: &Path,
    call: &str,
    name: &str,
    phase: &str,
    refusal: &SdkRefusal,
    uncertain: bool,
) {
    if uncertain {
        let _ = home::log_line(
            home,
            "uncertain.jsonl",
            &SdkValue::map([
                ("at", SdkValue::Int(home::now_ms())),
                ("call", SdkValue::text(call)),
                ("store", SdkValue::text(name)),
                ("phase", SdkValue::text(phase)),
                ("message", SdkValue::text(&refusal.message)),
            ]),
        );
    } else {
        let _ = mark(home, name, SdkValue::Null);
    }
}

pub(super) fn progress(context: &mut Context<'_>, phase: &str, done: u64, total: u64) {
    context.progress(SdkValue::map([
        ("phase", SdkValue::text(phase)),
        ("done", SdkValue::Int(done as i64)),
        ("total", SdkValue::Int(total as i64)),
    ]));
}

/// Sets or clears the record's `in_progress`, under the home lock.
pub(super) fn mark(home: &Path, name: &str, value: SdkValue) -> Result<()> {
    let _lock = home::lock::take(home)?;
    record_of(home, name)?
        .with("in_progress", value)
        .write(home)
}
