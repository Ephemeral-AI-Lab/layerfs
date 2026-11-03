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
//! `log/uncertain.jsonl`; nothing is cleaned up on a guess.
use crate::common::context::Context;
use crate::common::home::lease;
use crate::common::home::{self, paths, with_field};
use crate::common::outcome::{failed, refused, Outcome, View};
use crate::common::place::{self, Executor, Start};
use crate::machine::record::MachineRecord;
use crate::store::record::{StoreDerived, StoreRecord, StoreRef, StoreState};
use crate::store::transfer::steps::CHUNK;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use std::path::Path;

type Result<T> = std::result::Result<T, SdkRefusal>;

fn record_of(home: &Path, name: &str) -> Result<StoreRecord> {
    StoreRecord::of(home, name)
}

/// A Store as callers see it, with its projects (and a list item's
/// `state`): never the cursor key.
fn public(home: &Path, record: &StoreRecord, view: View, state: Option<StoreState>) -> Outcome {
    let projects = paths::names(&paths::store_dir(home, record.name()).join("project"));
    let derived = StoreDerived {
        projects: projects.map_err(failed)?,
        state,
    };
    record.to_sdk(view, &derived)
}

/// `store.list`: every Store with its state, derived as `store.status`
/// derives it (#536 §10b), starting nothing: the lease holders, read under
/// the home lock once, and for a sqlite Store whether its machine answers
/// a `stat` of its file. A machine that did not answer once is down for
/// its other Stores too, without dialing it again; a Store whose kind or
/// machine its record does not tell is `down`.
fn list(home: &Path) -> Outcome {
    let records = crate::inventory::stores(home)?;
    let holders = {
        let _lock = home::lock::take(home)?;
        let held = records.iter().map(|r| lease::holders(home, r.name()));
        held.collect::<Result<Vec<_>>>()?
    };
    let mut unanswered = std::collections::BTreeSet::new();
    let mut items = Vec::new();
    for (record, holders) in records.iter().zip(holders) {
        let down = match record.store_ref() {
            Err(_) => true,
            Ok(StoreRef::Postgres { .. }) => false,
            Ok(store_ref) => match store_ref.machine() {
                Err(_) => true,
                Ok(machine) if unanswered.contains(machine) => true,
                Ok(machine) => {
                    let stat = place::plan(home, store_ref, Start::Never).and_then(|mut on| {
                        step(home, &mut on, "stat", of_store(record.name(), vec![]))
                    });
                    if stat.is_err() {
                        unanswered.insert(machine.to_owned());
                    }
                    stat.is_err()
                }
            },
        };
        let state = record.state(down, &holders);
        items.push(public(home, record, View::List, Some(state))?);
    }
    Ok(SdkValue::map([("items", SdkValue::List(items))]))
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
fn step(home: &Path, executor: &mut Executor, op: &str, args: SdkValue) -> Outcome {
    match executor {
        Executor::Local => crate::store::transfer::steps::run(home, op, &args),
        Executor::Worker(worker) => {
            let request = SdkValue::map([("op", SdkValue::text(op)), ("args", args)]);
            worker.call("store.file", request, &mut |_| {})
        }
    }
}

fn fields_map(fields: Vec<(&str, SdkValue)>) -> SdkValue {
    SdkValue::Map(fields.into_iter().map(|(n, v)| (n.to_owned(), v)).collect())
}

fn of_store(name: &str, more: Vec<(&str, SdkValue)>) -> SdkValue {
    let mut fields = vec![("store", SdkValue::text(name))];
    fields.extend(more);
    fields_map(fields)
}

/// `store.status`: who holds it, whether a move is running, its size. It
/// starts no machine: a Store whose machine is not running is `down`, with
/// the reason (O34).
fn status(home: &Path, name: &str) -> Outcome {
    let record = record_of(home, name)?;
    let holders = {
        let _lock = home::lock::take(home)?;
        lease::holders(home, name)?
    };
    let mut down = None;
    let mut more = Vec::new();
    let store_ref = record.store_ref()?;
    if let StoreRef::Postgres { .. } = store_ref {
        more.push(("server", crate::store::connect::status(&record)));
    } else {
        more.push(("machine", SdkValue::text(store_ref.machine()?)));
        // the file's size where it lives, starting nothing (P4); a machine
        // that is down says why
        let file = place::plan(home, store_ref, Start::Never)
            .and_then(|mut executor| step(home, &mut executor, "stat", of_store(name, vec![])));
        let file = file.unwrap_or_else(|why| {
            down = Some(why.message.clone());
            SdkValue::map([("unavailable", SdkValue::Text(why.message))])
        });
        more.push(("file", file));
    }
    let state = record.state(down.is_some(), &holders).as_str();
    let mut fields = vec![
        ("name", SdkValue::text(name)),
        ("kind", SdkValue::text(store_ref.kind().as_str())),
        ("state", SdkValue::text(state)),
        (
            "holders",
            SdkValue::List(
                holders
                    .iter()
                    .map(|p| SdkValue::Int(i64::from(*p)))
                    .collect(),
            ),
        ),
        (
            "in_progress",
            record.in_progress().cloned().unwrap_or_default(),
        ),
        ("down_reason", down.map(SdkValue::Text).unwrap_or_default()),
    ];
    fields.extend(more);
    Ok(fields_map(fields))
}

/// `store.create`: an empty sqlite Store on `native`.
fn create(home: &Path, name: &str) -> Outcome {
    crate::common::names::StoreName::parse(name)?;
    let _lock = home::lock::take(home)?;
    if paths::store_record(home, name).exists() {
        return Err(refused("Exists", format!("a Store named {name} exists")));
    }
    let directory = paths::store_dir(home, name);
    home::private_dir(&directory)?;
    let identity = crate::store::identity::Identity {
        kind: crate::store::identity::Kind::Sqlite,
        binding: crate::sandbox::registry::random()?.to_vec(),
        cursor_key: crate::sandbox::registry::random()?,
    };
    let file = paths::store_file(home, name);
    let policy = layerfs_storage::StoragePolicy::frozen_default();
    let sealed = match crate::store::identity::make_empty(&directory, &file, &identity, policy) {
        Ok(sealed) => sealed,
        Err(refusal) => {
            // nothing was published: this call's own directory goes
            let _ = std::fs::remove_dir_all(&directory);
            return Err(refusal);
        }
    };
    crate::store::identity::save_record(home, name, &identity)?;
    let record = StoreRecord::read(home, name)?
        .unwrap_or_else(|| StoreRecord::from_record(name, SdkValue::Null))
        .created_on_native(&sealed.sqlite_version);
    record.write(home)?;
    public(home, &record, View::Full, None)
}

/// Refuses `InUse` while another process holds the Store, a Sandbox of
/// one of its projects runs, or a live process moves or removes it. Under
/// the home lock.
fn idle(home: &Path, name: &str, record: &StoreRecord) -> Result<()> {
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
fn settle(home: &Path, name: &str) -> Result<lease::Lease> {
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
fn in_progress(op: &str, more: Vec<(&str, SdkValue)>) -> SdkValue {
    let mut fields = vec![("op", SdkValue::text(op))];
    fields.extend(more);
    fields.extend(lease::identity());
    fields.push(("started", SdkValue::Int(home::now_ms())));
    fields_map(fields)
}

/// Ends a move or remove that failed after writing `in_progress`: a certain
/// failure clears it; an uncertain one keeps it and is logged.
fn ended(home: &Path, call: &str, name: &str, phase: &str, refusal: &SdkRefusal, uncertain: bool) {
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

/// `store.remove`: deletes a sqlite Store wherever it is, with every project
/// in it. Its backups stay in their buckets. A stopped machine is started
/// before the home lock is taken, never under it.
fn remove(home: &Path, name: &str) -> Outcome {
    let record = record_of(home, name)?;
    let store_ref = record.store_ref()?;
    if let StoreRef::Postgres { .. } = store_ref {
        return Err(refused(
            "InvalidInput",
            format!("Store {name} is a PostgreSQL database; `store disconnect` forgets it and keeps its data"),
        ));
    }
    let machine = store_ref.machine()?.to_owned();
    let mut executor = place::plan(home, store_ref, Start::OnDemand)?;
    if let Executor::Worker(worker) = &mut executor {
        worker.ready()?;
    }
    {
        let _lock = home::lock::take(home)?;
        let record = record_of(home, name)?;
        if record.machine()? != machine {
            return Err(refused(
                "Conflict",
                format!("Store {name} moved off {machine} meanwhile; nothing was removed"),
            ));
        }
        idle(home, name, &record)?;
        let at = in_progress("remove", vec![("machine", SdkValue::text(&machine))]);
        record.with("in_progress", at).write(home)?;
    }
    // a machine whose Stores open in this process keeps the file under the
    // home: it goes with the Store's directory below
    let removed = settle(home, name).and_then(|lease| {
        drop(lease);
        match executor {
            Executor::Local => Ok(SdkValue::Null),
            Executor::Worker(_) => step(home, &mut executor, "remove", of_store(name, vec![])),
        }
    });
    if let Err(refusal) = removed {
        let uncertain = refusal.code == "Unknown";
        ended(home, "store.remove", name, "remove", &refusal, uncertain);
        return Err(refusal);
    }
    let _lock = home::lock::take(home)?;
    std::fs::remove_dir_all(paths::store_dir(home, name)).map_err(failed)?;
    Ok(SdkValue::map([("removed", SdkValue::text(name))]))
}

fn progress(context: &mut Context<'_>, phase: &str, done: u64, total: u64) {
    context.progress(SdkValue::map([
        ("phase", SdkValue::text(phase)),
        ("done", SdkValue::Int(done as i64)),
        ("total", SdkValue::Int(total as i64)),
    ]));
}

/// Sets or clears the record's `in_progress`, under the home lock.
fn mark(home: &Path, name: &str, value: SdkValue) -> Result<()> {
    let _lock = home::lock::take(home)?;
    record_of(home, name)?
        .with("in_progress", value)
        .write(home)
}

/// `store.move`: the Store's file from its machine to `to`.
fn move_store(context: &mut Context<'_>, name: &str, to: &str) -> Outcome {
    let home = context.home.clone();
    crate::common::names::MachineName::parse(to)?;
    // check: names, the target machine, the Store idle
    progress(context, "check", 0, 0);
    let from = {
        let _lock = home::lock::take(&home)?;
        let record = record_of(&home, name)?;
        if let StoreRef::Postgres { .. } = record.store_ref()? {
            return Err(refused(
                "InvalidInput",
                format!("Store {name} is a PostgreSQL database; it does not move"),
            ));
        }
        if MachineRecord::read(&home, to)?.is_none() {
            return Err(refused("NotFound", format!("no machine named {to}")));
        }
        let from = record.machine()?;
        if from == to {
            return Err(refused(
                "InvalidInput",
                format!("Store {name} is already on {to}"),
            ));
        }
        idle(&home, name, &record)?;
        let at = in_progress(
            "move",
            vec![("from", SdkValue::text(&from)), ("to", SdkValue::text(to))],
        );
        record.with("in_progress", at).write(&home)?;
        from
    };
    let result =
        at("check", settle(&home, name)).and_then(|_lease| phases(context, name, &from, to));
    if let Err((phase, refusal)) = &result {
        // past install both copies exist: a record phase that failed (its
        // lock wait expired, say) leaves the outcome open
        let uncertain = refusal.code == "Unknown" || *phase == "record";
        ended(&home, "store.move", name, phase, refusal, uncertain);
    }
    result.map_err(|(phase, mut refusal)| {
        refusal.message = format!("store move, {phase}: {}", refusal.message);
        refusal.detail = with_field(refusal.detail, "phase", SdkValue::text(phase));
        refusal
    })
}

type Phased<T> = std::result::Result<T, (&'static str, SdkRefusal)>;

fn at<T>(phase: &'static str, result: Result<T>) -> Phased<T> {
    result.map_err(|refusal| (phase, refusal))
}

fn phases(context: &mut Context<'_>, name: &str, from: &str, to: &str) -> Phased<SdkValue> {
    let home = context.home.clone();
    let mut source = at("check", place::on_machine(&home, from, Start::OnDemand))?;
    let mut target = at("check", place::on_machine(&home, to, Start::OnDemand))?;
    for executor in [&mut source, &mut target] {
        if let Executor::Worker(worker) = executor {
            at("check", worker.ready())?;
        }
    }
    let exists = at(
        "check",
        step(&home, &mut target, "stat", of_store(name, vec![])),
    )?;
    if matches!(exists.get("exists"), Some(SdkValue::Bool(true))) {
        return Err((
            "check",
            refused(
                "Exists",
                format!("machine {to} already holds a Store file named {name}"),
            ),
        ));
    }
    // snapshot
    progress(context, "snapshot", 0, 0);
    let snapshot = at(
        "snapshot",
        step(&home, &mut source, "snapshot", of_store(name, vec![])),
    )?;
    let total = snapshot.get_int("bytes").unwrap_or(0) as u64;
    // send, in pieces through this process
    let mut offset = 0u64;
    progress(context, "send", 0, total);
    loop {
        let piece = at(
            "send",
            step(
                &home,
                &mut source,
                "read",
                of_store(
                    name,
                    vec![
                        ("offset", SdkValue::Int(offset as i64)),
                        ("len", SdkValue::Int(CHUNK as i64)),
                    ],
                ),
            ),
        )?;
        let bytes = match piece.get("bytes") {
            Some(SdkValue::Bytes(bytes)) => bytes.clone(),
            _ => Vec::new(),
        };
        if bytes.is_empty() && offset > 0 {
            break;
        }
        let len = bytes.len() as u64;
        at(
            "send",
            step(
                &home,
                &mut target,
                "write",
                of_store(
                    name,
                    vec![
                        ("offset", SdkValue::Int(offset as i64)),
                        ("bytes", SdkValue::Bytes(bytes)),
                    ],
                ),
            ),
        )?;
        offset += len;
        progress(context, "send", offset, total);
        if len < CHUNK {
            break;
        }
    }
    // verify, install
    progress(context, "verify", 0, 0);
    let digest: Vec<(&str, SdkValue)> = ["bytes", "sha256", "heads"]
        .into_iter()
        .map(|field| (field, snapshot.get(field).cloned().unwrap_or_default()))
        .collect();
    at(
        "verify",
        step(&home, &mut target, "verify", of_store(name, digest)),
    )?;
    progress(context, "install", 0, 0);
    at(
        "install",
        step(&home, &mut target, "install", of_store(name, vec![])),
    )?;
    // record: the Store is on `to` from here on
    progress(context, "record", 0, 0);
    let last = SdkValue::map([
        ("from", SdkValue::text(from)),
        ("to", SdkValue::text(to)),
        ("at", SdkValue::Int(home::now_ms())),
        ("bytes", SdkValue::Int(total as i64)),
        (
            "sha256",
            snapshot.get("sha256").cloned().unwrap_or_default(),
        ),
        ("heads", snapshot.get("heads").cloned().unwrap_or_default()),
    ]);
    at(
        "record",
        (|| {
            let _lock = home::lock::take(&home)?;
            let record = record_of(&home, name)?.with("machine", SdkValue::text(to));
            let record = record.with("last_move", last.clone());
            record.with("in_progress", SdkValue::Null).write(&home)
        })(),
    )?;
    // retire the source's copy
    progress(context, "retire", 0, 0);
    at(
        "retire",
        step(&home, &mut source, "retire", of_store(name, vec![])),
    )?;
    Ok(SdkValue::map([
        ("store", SdkValue::text(name)),
        ("from", SdkValue::text(from)),
        ("to", SdkValue::text(to)),
        ("bytes", SdkValue::Int(total as i64)),
        ("heads", snapshot.get("heads").cloned().unwrap_or_default()),
    ]))
}
