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
use crate::common::home::{self, paths, read_record, with_field, write_record};
use crate::common::outcome::{failed, refused, Outcome};
use crate::common::place;
use crate::machine::worker::Endpoint;
use crate::store::record::StoreRef;
use crate::store::resolve;
use crate::store::transfer::steps::CHUNK;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use std::path::Path;

type Result<T> = std::result::Result<T, SdkRefusal>;

fn record_of(home: &Path, name: &str) -> Result<SdkValue> {
    crate::common::names::name("store", name).map_err(|why| refused("InvalidInput", why))?;
    read_record(&paths::store_record(home, name))?
        .ok_or_else(|| refused("NotFound", format!("no Store named {name}")))
}

fn projects(home: &Path, name: &str) -> Result<Vec<String>> {
    paths::names(&paths::store_dir(home, name).join("project")).map_err(failed)
}

/// A record as callers see it: never the cursor key.
fn public(home: &Path, name: &str, record: &SdkValue) -> Result<SdkValue> {
    let SdkValue::Map(fields) = record else {
        return Err(refused(
            "Failed",
            format!("Store {name}'s record is not a record"),
        ));
    };
    let mut out: Vec<(String, SdkValue)> = fields
        .iter()
        .filter(|(field, _)| !matches!(field.as_str(), "cursor" | "binding"))
        .cloned()
        .collect();
    out.retain(|(field, _)| field != "name");
    out.insert(0, ("name".to_owned(), SdkValue::text(name)));
    let names = projects(home, name)?
        .into_iter()
        .map(SdkValue::Text)
        .collect();
    out.push(("projects".to_owned(), SdkValue::List(names)));
    Ok(SdkValue::Map(out))
}

/// Runs one sqlite `store` call.
pub fn run(context: &mut Context<'_>, call: &str, args: &SdkValue) -> Outcome {
    let home = context.home.clone();
    let name = args.get_text("name").unwrap_or("");
    match call {
        "list" => {
            let mut items = Vec::new();
            for store in paths::names(&home.join("store")).map_err(failed)? {
                if let Some(record) = read_record(&paths::store_record(&home, &store))? {
                    items.push(public(&home, &store, &record)?);
                }
            }
            Ok(SdkValue::map([("items", SdkValue::List(items))]))
        }
        "get" => public(&home, name, &record_of(&home, name)?),
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
fn step(home: &Path, endpoint: &Option<Endpoint>, op: &str, args: SdkValue) -> Outcome {
    match endpoint {
        None => crate::store::transfer::steps::run(home, op, &args),
        Some(endpoint) => {
            let request = SdkValue::map([("op", SdkValue::text(op)), ("args", args)]);
            crate::machine::worker::call(endpoint, "store.file", request, &mut |_| {})
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
    let moving = resolve::moving(&record, name).is_err();
    let mut down = None;
    let mut more = Vec::new();
    let store_ref = StoreRef::from_record(name, &record)?;
    if let StoreRef::Postgres { .. } = store_ref {
        more.push(("server", crate::store::connect::status(&record, name)));
    } else {
        let machine = store_ref.machine()?.to_owned();
        more.push(("machine", SdkValue::text(&machine)));
        // the file's size where it lives; a machine that is down says why
        let file = place::endpoint_now(home, &machine)
            .and_then(|endpoint| step(home, &endpoint, "stat", of_store(name, vec![])));
        let file = file.unwrap_or_else(|why| {
            down = Some(why.message.clone());
            SdkValue::map([("unavailable", SdkValue::Text(why.message))])
        });
        more.push(("file", file));
    }
    let state = if moving {
        "moving"
    } else if down.is_some() {
        "down"
    } else if holders.is_empty() {
        "idle"
    } else {
        "in_use"
    };
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
            record.get("in_progress").cloned().unwrap_or_default(),
        ),
        ("down_reason", down.map(SdkValue::Text).unwrap_or_default()),
    ];
    fields.extend(more);
    Ok(fields_map(fields))
}

/// `store.create`: an empty sqlite Store on `native`.
fn create(home: &Path, name: &str) -> Outcome {
    crate::common::names::name("store", name).map_err(|why| refused("InvalidInput", why))?;
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
    let path = paths::store_record(home, name);
    let mut record = read_record(&path)?.unwrap_or_default();
    for (field, value) in [
        ("machine", SdkValue::text("native")),
        ("profile", SdkValue::text("disposable")),
        ("created", SdkValue::Int(home::now_ms())),
        ("sqlite", SdkValue::text(&sealed.sqlite_version)),
    ] {
        record = with_field(record, field, value);
    }
    write_record(&path, &record)?;
    public(home, name, &record)
}

/// The Sandboxes of any machine whose project is in Store `name`.
fn sandboxes_of(home: &Path, name: &str) -> Result<Vec<String>> {
    let mut found = Vec::new();
    for machine in paths::names(&home.join("machine")).map_err(failed)? {
        let dir = paths::machine_dir(home, &machine).join("sandbox");
        for sandbox in paths::names(&dir).map_err(failed)? {
            let record = read_record(&dir.join(&sandbox).join("record.json"))?;
            let project = record
                .as_ref()
                .and_then(|r| r.get_text("project"))
                .unwrap_or("");
            if project.split_once('/').map(|(s, _)| s) == Some(name) {
                found.push(sandbox);
            }
        }
    }
    Ok(found)
}

/// Refuses `InUse` while another process holds the Store, a Sandbox of
/// one of its projects runs, or a live process moves or removes it. Under
/// the home lock.
fn idle(home: &Path, name: &str, record: &SdkValue) -> Result<()> {
    resolve::moving(record, name)?;
    let holders = lease::holders(home, name)?;
    if !holders.is_empty() {
        return Err(refused(
            "InUse",
            format!("Store {name} is open in process {holders:?}"),
        ));
    }
    let sandboxes = sandboxes_of(home, name)?;
    if !sandboxes.is_empty() {
        return Err(refused(
            "InUse",
            format!(
                "Store {name} has running Sandboxes: {}",
                sandboxes.join(", ")
            ),
        ));
    }
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
    let sandboxes = sandboxes_of(home, name)?;
    if !sandboxes.is_empty() {
        return Err(refused(
            "InUse",
            format!(
                "Store {name} has running Sandboxes: {}",
                sandboxes.join(", ")
            ),
        ));
    }
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
    let store_ref = StoreRef::from_record(name, &record)?;
    if let StoreRef::Postgres { .. } = store_ref {
        return Err(refused(
            "InvalidInput",
            format!("Store {name} is a PostgreSQL database; `store disconnect` forgets it and keeps its data"),
        ));
    }
    let machine = store_ref.machine()?.to_owned();
    let endpoint = match machine.as_str() {
        "native" => None,
        _ => place::endpoint(home, &machine)?,
    };
    {
        let _lock = home::lock::take(home)?;
        let record = record_of(home, name)?;
        if resolve::machine(&record, name)? != machine {
            return Err(refused(
                "Conflict",
                format!("Store {name} moved off {machine} meanwhile; nothing was removed"),
            ));
        }
        idle(home, name, &record)?;
        let at = in_progress("remove", vec![("machine", SdkValue::text(&machine))]);
        write_record(
            &paths::store_record(home, name),
            &with_field(record, "in_progress", at),
        )?;
    }
    // a machine whose Stores open in this process keeps the file under the
    // home: it goes with the Store's directory below
    let removed = settle(home, name).and_then(|lease| {
        drop(lease);
        match endpoint {
            None => Ok(SdkValue::Null),
            Some(_) => step(home, &endpoint, "remove", of_store(name, vec![])),
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
    let path = paths::store_record(home, name);
    let record = record_of(home, name)?;
    write_record(&path, &with_field(record, "in_progress", value))
}

/// `store.move`: the Store's file from its machine to `to`.
fn move_store(context: &mut Context<'_>, name: &str, to: &str) -> Outcome {
    let home = context.home.clone();
    crate::common::names::name("machine", to).map_err(|why| refused("InvalidInput", why))?;
    // check: names, the target machine, the Store idle
    progress(context, "check", 0, 0);
    let from = {
        let _lock = home::lock::take(&home)?;
        let record = record_of(&home, name)?;
        if let StoreRef::Postgres { .. } = StoreRef::from_record(name, &record)? {
            return Err(refused(
                "InvalidInput",
                format!("Store {name} is a PostgreSQL database; it does not move"),
            ));
        }
        if read_record(&paths::machine_record(&home, to))?.is_none() {
            return Err(refused("NotFound", format!("no machine named {to}")));
        }
        let from = resolve::machine(&record, name)?;
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
        write_record(
            &paths::store_record(&home, name),
            &with_field(record, "in_progress", at),
        )?;
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
    let source = at("check", place::endpoint(&home, from))?;
    let target = at("check", place::endpoint(&home, to))?;
    let exists = at(
        "check",
        step(&home, &target, "stat", of_store(name, vec![])),
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
        step(&home, &source, "snapshot", of_store(name, vec![])),
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
                &source,
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
                &target,
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
        step(&home, &target, "verify", of_store(name, digest)),
    )?;
    progress(context, "install", 0, 0);
    at(
        "install",
        step(&home, &target, "install", of_store(name, vec![])),
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
            let path = paths::store_record(&home, name);
            let record = record_of(&home, name)?;
            let record = with_field(record, "machine", SdkValue::text(to));
            let record = with_field(record, "last_move", last.clone());
            write_record(&path, &with_field(record, "in_progress", SdkValue::Null))
        })(),
    )?;
    // retire the source's copy
    progress(context, "retire", 0, 0);
    at(
        "retire",
        step(&home, &source, "retire", of_store(name, vec![])),
    )?;
    Ok(SdkValue::map([
        ("store", SdkValue::text(name)),
        ("from", SdkValue::text(from)),
        ("to", SdkValue::text(to)),
        ("bytes", SdkValue::Int(total as i64)),
        ("heads", snapshot.get("heads").cloned().unwrap_or_default()),
    ]))
}
