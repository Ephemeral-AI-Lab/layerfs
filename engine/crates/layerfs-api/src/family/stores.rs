//! The sqlite half of the `store` family (#533 step 8, H §7.3): list, get,
//! status, create, remove and move. `move` and `remove` read leases under
//! the home lock and refuse `InUse`. A move runs its phases once each
//! (check, snapshot, send, verify, install, record, retire); a failure names
//! its phase and keeps whatever exists. An uncertain step (`Unknown`) keeps
//! both copies, leaves the record's `in_progress` and is logged to
//! `log/uncertain.jsonl`; nothing is cleaned up on a guess.
use crate::calls::Context;
use crate::home::{self, paths, read_record, with_field, write_record};
use crate::host::outcome::{failed, refused, Outcome};
use crate::store::files::CHUNK;
use crate::store::{lease, route};
use crate::worker::Endpoint;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use std::path::Path;

type Result<T> = std::result::Result<T, SdkRefusal>;

fn record_of(home: &Path, name: &str) -> Result<SdkValue> {
    crate::names::name("store", name).map_err(|why| refused("InvalidInput", why))?;
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
pub(crate) fn run(context: &mut Context<'_>, call: &str, args: &SdkValue) -> Outcome {
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
        None => crate::store::files::run(home, op, &args),
        Some(endpoint) => {
            let request = SdkValue::map([("op", SdkValue::text(op)), ("args", args)]);
            crate::worker::call(endpoint, "store.file", request, &mut |_| {})
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

/// `store.status`: who holds it, whether a move is running, its size.
fn status(home: &Path, name: &str) -> Outcome {
    let record = record_of(home, name)?;
    let holders = {
        let _lock = home::lock::take(home)?;
        lease::holders(home, name)?
    };
    let moving = route::moving(&record, name).is_err();
    let state = if moving {
        "moving"
    } else if holders.is_empty() {
        "idle"
    } else {
        "in_use"
    };
    let mut fields = vec![
        ("name", SdkValue::text(name)),
        (
            "kind",
            SdkValue::text(record.get_text("kind").unwrap_or("sqlite")),
        ),
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
    ];
    if record.get_text("kind") != Some("postgresql") {
        let machine = route::machine(&record, name)?;
        fields.push(("machine", SdkValue::text(&machine)));
        // the file's size where it lives; a machine that is down says why
        let file = route::endpoint(home, &machine)
            .and_then(|endpoint| step(home, &endpoint, "stat", of_store(name, vec![])));
        fields.push((
            "file",
            file.unwrap_or_else(|why| {
                SdkValue::map([("unavailable", SdkValue::Text(why.message))])
            }),
        ));
    }
    Ok(fields_map(fields))
}

/// `store.create`: an empty sqlite Store on `native`.
fn create(home: &Path, name: &str) -> Outcome {
    crate::names::name("store", name).map_err(|why| refused("InvalidInput", why))?;
    let _lock = home::lock::take(home)?;
    if paths::store_record(home, name).exists() {
        return Err(refused("Exists", format!("a Store named {name} exists")));
    }
    let directory = paths::store_dir(home, name);
    home::private_dir(&directory)?;
    let identity = crate::host::stores::Identity {
        kind: crate::host::stores::Kind::Sqlite,
        binding: crate::host::registry::random()?.to_vec(),
        cursor_key: crate::host::registry::random()?,
    };
    let file = paths::store_file(home, name);
    let policy = layerfs_storage::StoragePolicy::frozen_default();
    let sealed = match crate::host::stores::make_empty(&directory, &file, &identity, policy) {
        Ok(sealed) => sealed,
        Err(refusal) => {
            // nothing was published: this call's own directory goes
            let _ = std::fs::remove_dir_all(&directory);
            return Err(refusal);
        }
    };
    crate::host::stores::save_record(home, name, &identity)?;
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
/// one of its projects runs, or a live process moves it. Under the home lock.
fn idle(home: &Path, name: &str, record: &SdkValue) -> Result<()> {
    route::moving(record, name)?;
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

/// `store.remove`: deletes a sqlite Store wherever it is, with every project
/// in it. Its backups stay in their buckets.
fn remove(home: &Path, name: &str) -> Outcome {
    let _lock = home::lock::take(home)?;
    let record = record_of(home, name)?;
    if record.get_text("kind") == Some("postgresql") {
        return Err(refused(
            "InvalidInput",
            format!("Store {name} is a PostgreSQL database; `store disconnect` forgets it and keeps its data"),
        ));
    }
    idle(home, name, &record)?;
    let machine = route::machine(&record, name).unwrap_or_else(|_| "native".to_owned());
    if machine != "native" {
        let endpoint = route::endpoint(home, &machine)?;
        step(home, &endpoint, "remove", of_store(name, vec![]))?;
    }
    let directory = paths::store_dir(home, name);
    std::fs::remove_dir_all(&directory).map_err(failed)?;
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
    crate::names::name("machine", to).map_err(|why| refused("InvalidInput", why))?;
    // check: names, the target machine, the Store idle
    progress(context, "check", 0, 0);
    let from = {
        let _lock = home::lock::take(&home)?;
        let record = record_of(&home, name)?;
        if record.get_text("kind") == Some("postgresql") {
            return Err(refused(
                "InvalidInput",
                format!("Store {name} is a PostgreSQL database; it does not move"),
            ));
        }
        if read_record(&paths::machine_record(&home, to))?.is_none() {
            return Err(refused("NotFound", format!("no machine named {to}")));
        }
        let from = match record.get("machine") {
            Some(SdkValue::Text(machine)) => machine.clone(),
            None => "native".to_owned(),
            Some(_) => {
                return Err(refused(
                    "Unavailable",
                    format!(
                        "Store {name}'s machine is not known; run `layerfs store import-env` first"
                    ),
                ))
            }
        };
        if from == to {
            return Err(refused(
                "InvalidInput",
                format!("Store {name} is already on {to}"),
            ));
        }
        idle(&home, name, &record)?;
        let at = SdkValue::map([
            ("from", SdkValue::text(&from)),
            ("to", SdkValue::text(to)),
            ("pid", SdkValue::Int(i64::from(std::process::id()))),
            ("started", SdkValue::Int(home::now_ms())),
        ]);
        write_record(
            &paths::store_record(&home, name),
            &with_field(record, "in_progress", at),
        )?;
        from
    };
    let _lease = lease::hold(&home, name)?;
    let result = phases(context, name, &from, to);
    match &result {
        Ok(_) => {}
        Err((phase, refusal)) if refusal.code == "Unknown" => {
            let _ = home::log_line(
                &home,
                "uncertain.jsonl",
                &SdkValue::map([
                    ("at", SdkValue::Int(home::now_ms())),
                    ("call", SdkValue::text("store.move")),
                    ("store", SdkValue::text(name)),
                    ("from", SdkValue::text(&from)),
                    ("to", SdkValue::text(to)),
                    ("phase", SdkValue::text(*phase)),
                    ("message", SdkValue::text(&refusal.message)),
                ]),
            );
        }
        Err(_) => {
            let _ = mark(&home, name, SdkValue::Null);
        }
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
    let source = at("check", route::endpoint(&home, from))?;
    let target = at("check", route::endpoint(&home, to))?;
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
