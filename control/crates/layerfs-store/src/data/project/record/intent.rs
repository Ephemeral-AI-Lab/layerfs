//! The operation a project's record holds while an Init or a delete of the
//! project runs (call surface 2.0): the `op` of `layerfs_home::record::intent`, in
//! `store/<store>/project/<name>/record.json`, so `project list` and
//! `project get` can answer `busy` while it runs and `broken` `unresolved`
//! once its owner is gone with its outcome unknown.
//!
//! An Init claims the project's entry before its first effect: the folder
//! and a record holding the Init's `op`, under the home's lock, so a project
//! being imported is listed (`busy`) and a second Init of the name is
//! refused `NameTaken` at once. Success writes the finished record without
//! the `op`; a certain failure removes the entry; an uncertain one keeps it
//! with the `op` marked uncertain. A delete records its `op` on the record
//! it deletes: success removes the entry with it, a certain failure drops
//! the `op`, an uncertain one keeps it marked uncertain.
use crate::data::project::init::sqlite::make_entry;
use crate::data::project::record::project::ProjectRecord;
use layerfs_home::disk::paths;
use layerfs_home::lock::home;
use layerfs_home::record::intent::{Op, Standing, FIELD};
use layerfs_protocol::sdk::{SdkRefusal, SdkValue};
use std::path::Path;

type Result<T> = std::result::Result<T, SdkRefusal>;

/// The operation name an Init records.
pub const INIT: &str = "project.init";
/// The operation name a delete records.
pub const DELETE: &str = "project.delete";

/// The record of project `name` of Store `store` with `op` set.
fn holding(record: ProjectRecord, op: &Op) -> ProjectRecord {
    record.with(FIELD, op.to_record().clone())
}

/// Whether the record of project `name` of Store `store` holds an Init
/// this process runs: the entry an Init of this process claimed, which its
/// add then fills.
pub fn claimed(home: &Path, store: &str, name: &str) -> Result<bool> {
    let record = ProjectRecord::read(home, store, name)?;
    let op = record.and_then(|record| record.op());
    Ok(op.is_some_and(|op| op.name() == INIT && op.mine()))
}

/// Claims project `name` of Store `store` for an Init of this process:
/// under the home's lock its entry folder is made and its record written
/// holding the Init's `op` at `phase`. An entry already there is
/// `NameTaken` with its `home` source.
pub fn claim(home: &Path, store: &str, name: &str, phase: &str) -> Result<()> {
    let _lock = home::take(home)?;
    make_entry(home, store, name)?;
    let op = Op::begin(INIT, phase, SdkValue::Null)?;
    let fields = SdkValue::map([("name", SdkValue::text(name))]);
    let record = ProjectRecord::from_record(store, name, fields);
    holding(record, &op).write(home)
}

/// Begins operation `call` (a delete) at `phase` on the record of project
/// `name` of Store `store`, when there is one: refused `Busy` while
/// another live process's operation holds it; one whose owner is gone is
/// replaced (running the delete again is what clears it). Answers whether
/// a record now holds it.
pub fn begin(home: &Path, store: &str, name: &str, call: &str, phase: &str) -> Result<bool> {
    let _lock = home::take(home)?;
    let Some(record) = ProjectRecord::read(home, store, name)? else {
        return Ok(false);
    };
    if let Some(held) = record.op() {
        if held.standing() == Standing::Running && !held.mine() {
            let project = format!("{store}/{name}");
            held.blocks("project", &project, "project get", "project delete")?;
        }
    }
    holding(record, &Op::begin(call, phase, SdkValue::Null)?).write(home)?;
    Ok(true)
}

/// Moves the operation this process holds on the project to `phase`.
pub fn phase(home: &Path, store: &str, name: &str, phase: &str) -> Result<()> {
    let _lock = home::take(home)?;
    let Some(record) = ProjectRecord::read(home, store, name)? else {
        return Ok(());
    };
    match record.op() {
        Some(op) if op.mine() => holding(record, &op.at(phase)).write(home),
        _ => Ok(()),
    }
}

/// Moves the operation this process holds on the project to `phase`,
/// recording `args` with it: what settling it needs once its owner is gone
/// (an Init's folder copy on a machine, `{ copy, machine }`).
pub fn phase_with(home: &Path, store: &str, name: &str, phase: &str, args: SdkValue) -> Result<()> {
    let _lock = home::take(home)?;
    let Some(record) = ProjectRecord::read(home, store, name)? else {
        return Ok(());
    };
    match record.op() {
        Some(op) if op.mine() => holding(record, &op.at(phase).with_args(args)).write(home),
        _ => Ok(()),
    }
}

/// Drops the operation this process holds from the project's record, when
/// the record is still there.
pub fn end(home: &Path, store: &str, name: &str) -> Result<()> {
    let _lock = home::take(home)?;
    let Some(record) = ProjectRecord::read(home, store, name)? else {
        return Ok(());
    };
    match record.op() {
        Some(op) if op.mine() => record.without(FIELD).write(home),
        _ => Ok(()),
    }
}

/// Marks the operation this process holds on the project uncertain, and
/// when `log` appends it to `log/uncertain.jsonl` (an Init through a worker
/// has logged its copy's line already): its owner's end leaves the project
/// `broken` `unresolved`, fixed by `project delete`.
pub fn uncertain(home: &Path, store: &str, name: &str, message: &str, log: bool) -> Result<()> {
    let _lock = home::take(home)?;
    let Some(record) = ProjectRecord::read(home, store, name)? else {
        return Ok(());
    };
    match record.op() {
        Some(op) if op.mine() => {
            let op = op.as_uncertain();
            if log {
                op.log_uncertain(home, "project", &format!("{store}/{name}"), message);
            }
            holding(record, &op).write(home)
        }
        _ => Ok(()),
    }
}

/// Removes the entry an Init of this process claimed, after a certain
/// failure: only while its record still holds that Init's `op`.
pub fn abandon(home: &Path, store: &str, name: &str) -> Result<()> {
    let _lock = home::take(home)?;
    match claimed(home, store, name)? {
        true => remove_entry(home, store, name),
        false => Ok(()),
    }
}

/// Removes project `name` of Store `store`'s home entry, record and all:
/// what `project delete` does to the entry of an Init that never reached
/// the Store. An entry already gone is removed.
pub fn remove_entry(home: &Path, store: &str, name: &str) -> Result<()> {
    let _lock = home::take(home)?;
    let entry = paths::project_dir(home, store, name);
    match std::fs::remove_dir_all(&entry) {
        Ok(()) => {
            layerfs_home::record::generation::moved_above(&entry);
            Ok(())
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(layerfs_home::reply::outcome::failed(error)),
    }
}

/// Settles an Init's claim by its outcome. Success keeps what the Init
/// wrote. A refusal before the Init reached the Store (its `op` still at
/// `check` or `send`) is certain whatever its code, and removes the
/// claimed entry. One after it (`install`) is uncertain when it is
/// `Unknown`, or `Failed` when `local` (a local add keeps its entry for
/// diagnosis on a failure it cannot classify): its `op` is marked
/// uncertain, and logged when `local` (a worker's Init logged its copy's
/// line). Any other refusal is certain and removes the entry.
pub fn settle_init<T>(home: &Path, store: &str, name: &str, outcome: &Result<T>, local: bool) {
    let Err(refusal) = outcome else {
        return;
    };
    let record = ProjectRecord::read(home, store, name).ok().flatten();
    let op = record.and_then(|record| record.op());
    let installing = op.is_some_and(|op| op.name() == INIT && op.mine() && op.phase() == "install");
    let unknown = match refusal.code.as_str() {
        "Unknown" => true,
        "Failed" => local,
        _ => false,
    };
    let _ = match installing && unknown {
        true => uncertain(home, store, name, &refusal.message, local),
        false => abandon(home, store, name),
    };
}
