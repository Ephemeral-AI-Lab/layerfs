//! `store.create`: an empty sqlite Store on a machine whose calls run in
//! this process (its file on this computer), or made on a machine through
//! its worker.
//!
//! In this process the Store's file is made under the home lock and its
//! record written after it. On a machine the name is taken first: under
//! the home lock the record is written with the operation (`op`, name
//! `store.create`, uncertain: a crash from here leaves the file's fate
//! unknown), then, past the lock, the machine's worker makes the file
//! (`store.file` `create`), and the record drops the operation. A refusal
//! the worker made takes the record away again; a lost reply keeps it
//! `broken`, `unresolved`, and `store remove` clears it on both sides.
use crate::admin::record::identity::{self, Identity, MadeDirectory};
use crate::admin::record::intent;
use crate::admin::record::store::{self, StoreRecord};
use crate::admin::route::executor::{self, Executor};
use crate::backend::sqlite::family::step;
use crate::backend::sqlite::step_request::{StepFrame, StoreStep};
use layerfs_home::disk;
use layerfs_home::disk::paths;
use layerfs_home::record::intent::{self as op, Op};
use layerfs_home::reply::outcome::{refused, Outcome};
use layerfs_home::reply::time;
use layerfs_machine::record::machine::MachineRecord;
use layerfs_protocol::sdk::{SdkRefusal, SdkValue};
use std::path::Path;

type Result<T> = std::result::Result<T, SdkRefusal>;

/// A new Store's identity: its binding and cursor key, made here and kept
/// in the home's record.
fn new_identity() -> Result<Identity> {
    Ok(Identity {
        kind: identity::Kind::Sqlite,
        binding: layerfs_home::record::token::random()?.to_vec(),
        cursor_key: layerfs_home::record::token::random()?,
    })
}

/// What `store.create` answers: `{ outcome, store, engine, machine, profile,
/// sqlite_version, created_at }`.
fn created(record: &StoreRecord) -> SdkValue {
    let at = record.to_record().get_int("created");
    SdkValue::map([
        ("outcome", SdkValue::text("created")),
        ("store", SdkValue::text(record.name())),
        ("engine", SdkValue::text("sqlite")),
        ("machine", SdkValue::opt_text(record.text("machine"))),
        (
            "profile",
            SdkValue::opt_text(crate::admin::record::store::profile(record)),
        ),
        ("sqlite_version", SdkValue::opt_text(record.text("sqlite"))),
        (
            "created_at",
            at.map_or(SdkValue::Null, |ms| SdkValue::Text(time::iso(ms))),
        ),
    ])
}

/// `store.create`: an empty sqlite Store named `name` on `machine`; when
/// none is named, on the machine whose calls run in this process (`native`
/// on macOS and Linux), and `Unsupported` on a host with none: no other
/// machine is chosen. A name a Store has is `Exists`; a machine the home
/// does not record (a built-in one exists without a record) is `NotFound`;
/// one that is not running is `Unavailable`, naming `layerfs machine
/// start`: nothing is started.
pub fn create(home: &Path, name: &str, machine: Option<&str>) -> Outcome {
    let machine = match machine {
        Some(machine) => machine,
        None => layerfs_machine::in_process().ok_or_else(|| {
            layerfs_machine::none_in_process("a sqlite Store made without --machine")
        })?,
    };
    if !layerfs_home::names::BUILT_IN.contains(&machine)
        && MachineRecord::read(home, machine)?.is_none()
    {
        return Err(refused("NotFound", format!("no machine named {machine}")));
    }
    if paths::store_record(home, name).exists() {
        return Err(refused("Exists", format!("a Store named {name} exists")));
    }
    let mut executor = executor::on_machine(home, machine)?;
    if let Executor::Worker(worker) = &mut executor {
        worker.ready()?;
    }
    match executor {
        Executor::Local => here(home, name, machine),
        Executor::Worker(_) => on_machine(home, name, machine, &mut executor),
    }
}

/// The Store made in this process, under the home lock.
fn here(home: &Path, name: &str, machine: &str) -> Outcome {
    let _lock = layerfs_home::lock::home::take(home)?;
    if paths::store_record(home, name).exists() {
        return Err(refused("Exists", format!("a Store named {name} exists")));
    }
    let directory = paths::store_dir(home, name);
    if let Some(parent) = directory.parent() {
        disk::private_dir(parent)?;
    }
    let identity = new_identity()?;
    let file = paths::store_file(home, name);
    let policy = layerfs_storage::StoragePolicy::frozen_default();
    let (made_file, made) = identity::make_empty(&directory, &file, &identity, policy);
    let sealed = match made_file {
        Ok(sealed) => sealed,
        Err(refusal) => {
            // nothing was published: the directory goes only when this call's
            // own mkdir made it, never one another caller made meanwhile
            if made == MadeDirectory::ThisCall {
                let _ = std::fs::remove_dir_all(&directory);
            }
            return Err(refusal);
        }
    };
    identity::save_record(home, name, &identity)?;
    let record = StoreRecord::read(home, name)?
        .unwrap_or_else(|| StoreRecord::from_record(name, SdkValue::Null))
        .created_on(machine)
        .with("sqlite", SdkValue::text(&sealed.sqlite_version));
    record.write(home)?;
    Ok(created(&record))
}

/// The Store made by `machine`'s worker: the record first, with the
/// operation, then the file, then the record without it.
fn on_machine(home: &Path, name: &str, machine: &str, executor: &mut Executor) -> Outcome {
    let identity = new_identity()?;
    let args = SdkValue::map([("machine", SdkValue::text(machine))]);
    let op = Op::begin(intent::CREATE, "create", args)?.as_uncertain();
    let directory = paths::store_dir(home, name);
    let (record, made) = {
        let _lock = layerfs_home::lock::home::take(home)?;
        if paths::store_record(home, name).exists() {
            return Err(refused("Exists", format!("a Store named {name} exists")));
        }
        let made = !directory.exists();
        let fields = SdkValue::map([
            ("name", SdkValue::text(name)),
            ("kind", SdkValue::text("sqlite")),
            ("binding", SdkValue::Bytes(identity.binding.clone())),
            ("cursor", SdkValue::Bytes(identity.cursor_key.to_vec())),
        ]);
        let record = StoreRecord::from_record(name, fields).created_on(machine);
        record
            .clone()
            .with(op::FIELD, op.to_record().clone())
            .write(home)?;
        (record, made)
    };
    let frame = StepFrame {
        store: name.to_owned(),
        store_record: Some(record.to_record().clone()),
        step: StoreStep::Create,
    };
    match step(home, executor, &frame) {
        Ok(file) => {
            let _store = store::lock(home, name)?;
            let record = StoreRecord::of(home, name)?
                .without(op::FIELD)
                .with("sqlite", file.get("sqlite").cloned().unwrap_or_default());
            record.write(home)?;
            Ok(created(&record))
        }
        Err(refusal) if refusal.code == "Unknown" => {
            // the file's fate is not known: the record keeps the operation
            op.log_uncertain(home, "store", name, &refusal.message);
            Err(refusal)
        }
        Err(refusal) => {
            // the worker made nothing: the name is free again
            forget(home, name, &op, made);
            Err(refusal)
        }
    }
}

/// Takes back the record a refused create wrote, while it still holds
/// that create's operation: the Store's folder when this call made it,
/// else only the record.
fn forget(home: &Path, name: &str, op: &Op, made: bool) {
    let Ok(_lock) = layerfs_home::lock::home::take(home) else {
        return;
    };
    let Ok(_store) = store::lock(home, name) else {
        return;
    };
    let ours = StoreRecord::read(home, name)
        .ok()
        .flatten()
        .and_then(|record| record.op())
        .is_some_and(|held| held.id() == op.id());
    if !ours {
        return;
    }
    let _ = match made {
        true => std::fs::remove_dir_all(paths::store_dir(home, name)),
        false => std::fs::remove_file(paths::store_record(home, name)),
    };
    layerfs_home::record::generation::moved(home);
}
