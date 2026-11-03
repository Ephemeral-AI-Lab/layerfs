//! `store.push` and `store.pull` (#536 §7.3, H1): a sqlite Store's file
//! between `native` and a machine, one phase at a time (check, snapshot,
//! send, verify, install, record, retire). A push sends it from `native`
//! to a machine's worker, a pull from its machine's worker to `native`:
//! the phases are the same, only the sides swap. Between two machines the
//! caller pulls, then pushes; each half is its own operation.
use crate::common::context::Context;
use crate::common::home::{self, with_field};
use crate::common::op::{self, Op};
use crate::common::outcome::{refused, Outcome};
use crate::common::place::{self, Executor};
use crate::inventory;
use crate::store::family::{advance, begin, ended, of_store, progress, record_of, settle, step};
use crate::store::record::{self, StoreRecord};
use crate::store::transfer::resolve::retired;
use crate::store::transfer::steps::CHUNK;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use std::path::Path;

type Result<T> = std::result::Result<T, SdkRefusal>;

/// `store.push`: the Store's file from `native` to machine `to`.
pub(crate) fn push(context: &mut Context<'_>, name: &str, to: &str) -> Outcome {
    crate::common::names::MachineName::parse(to)?;
    transfer(context, name, Some(to))
}

/// `store.pull`: the Store's file from its machine to `native`.
pub(crate) fn pull(context: &mut Context<'_>, name: &str) -> Outcome {
    transfer(context, name, None)
}

/// Where the Store goes from and to: a push (`to` named) from `native`, a
/// pull from the machine its record names.
fn sides(home: &Path, record: &StoreRecord, to: Option<&str>) -> Result<(String, String)> {
    match to {
        Some(to) => {
            inventory::can_push(home, record, to)?;
            Ok(("native".to_owned(), to.to_owned()))
        }
        None => Ok((inventory::can_pull(record)?, "native".to_owned())),
    }
}

fn transfer(context: &mut Context<'_>, name: &str, target: Option<&str>) -> Outcome {
    let home = context.home.clone();
    let call = match target {
        Some(_) => "store.push",
        None => "store.pull",
    };
    let mut written = None;
    let result = run(context, name, target, call, &mut written);
    if let (Err((phase, refusal)), Some(op)) = (&result, &written) {
        // past install both copies exist: a record phase that failed (its
        // lock wait expired, say) leaves the outcome open; a certain install
        // failure put nothing in place; past record the Store is on `to`,
        // and a retire that did not finish leaves `last_transfer` saying so
        let uncertain = *phase != "retire" && (refusal.code == "Unknown" || *phase == "record");
        ended(&home, name, op, phase, refusal, uncertain, None);
    }
    let verb = call.replace('.', " ");
    result.map_err(|(phase, mut refusal)| {
        refusal.message = format!("{verb}, {phase}: {}", refusal.message);
        refusal.detail = with_field(refusal.detail, "phase", SdkValue::text(phase));
        refusal
    })
}

/// The check phase, then the rest once `op` is written into `written`.
fn run(
    context: &mut Context<'_>,
    name: &str,
    target: Option<&str>,
    call: &str,
    written: &mut Option<Op>,
) -> Phased<SdkValue> {
    let home = context.home.clone();
    // check, before `op` and outside the locks: the Store's side, both
    // machines, the target's file and its free disk; the side again under
    // the locks, where `op` is written
    progress(context, "check", None);
    // first the copy an ended push or pull left, which this call clears
    at("check", super::resolve::resolve(&home, name))?;
    let record = at("check", record_of(&home, name))?;
    at("check", record.unblocked())?;
    let (from, to) = at("check", sides(&home, &record, target))?;
    // what holds the Store is refused before any machine is reached
    at("check", held_now(&home, name))?;
    let (mut source, mut target_side) = at("check", reach(&home, name, &from, &to))?;
    let args = SdkValue::map([("from", SdkValue::text(&from)), ("to", SdkValue::text(&to))]);
    let op = at("check", Op::begin(call, "check", args))?;
    let begun = begin(&home, name, &op, |record| {
        record.unblocked()?;
        if sides(&home, record, target)? != (from.clone(), to.clone()) {
            return Err(refused(
                "Conflict",
                format!("Store {name} moved meanwhile; nothing was sent"),
            ));
        }
        Ok(())
    });
    at("check", begun)?;
    let op = written.insert(op);
    let _lease = at("check", settle(&home, name))?;
    phases(
        context,
        op,
        name,
        (&from, &mut source),
        (&to, &mut target_side),
    )
}

/// Refuses `InUse` while leases or Sandbox records hold Store `name`,
/// read under the Store's lock (stale leases are dropped there). The
/// check is repeated where `op` is written.
fn held_now(home: &Path, name: &str) -> Result<()> {
    let _store = crate::store::record::lock(home, name)?;
    crate::inventory::store_idle(home, name)
}

/// The headroom a target keeps beyond the Store's bytes: at least 1 GiB,
/// and a tenth of the Store when that is more.
pub(crate) const HEADROOM: u64 = 1 << 30;

/// Both machines' executors, once each answers: the target holds no Store
/// file named `name` and has the Store's bytes (with its write-ahead log)
/// plus [`HEADROOM`] free (#536 §7.3, §11 item 7). A stopped machine is
/// `Unavailable`, naming `layerfs machine start` (#535 D11).
fn reach(home: &Path, name: &str, from: &str, to: &str) -> Result<(Executor, Executor)> {
    let mut source = place::on_machine(home, from)?;
    let mut target = place::on_machine(home, to)?;
    for executor in [&mut source, &mut target] {
        if let Executor::Worker(worker) = executor {
            worker.ready()?;
        }
    }
    let held = step(home, &mut source, "stat", of_store(name, vec![]))?;
    let there = step(home, &mut target, "stat", of_store(name, vec![]))?;
    if matches!(there.get("exists"), Some(SdkValue::Bool(true))) {
        return Err(refused(
            "Exists",
            format!("machine {to} already holds a Store file named {name}"),
        ));
    }
    let size = |field| held.get_int(field).unwrap_or(0).max(0) as u64;
    let bytes = size("bytes").saturating_add(size("wal_bytes"));
    let Some(free) = there.get_int("free") else {
        return Err(refused(
            "Unsupported",
            format!("machine {to}'s worker does not report its free disk (an older bundle)"),
        ));
    };
    let headroom = HEADROOM.max(bytes / 10);
    let needed = bytes.saturating_add(headroom);
    let free = free.max(0) as u64;
    if free < needed {
        let mut refusal = refused(
            "Capacity",
            format!("machine {to} has {free} bytes free; Store {name} needs {needed} ({bytes} bytes and {headroom} of headroom)"),
        );
        let int = |n: u64| SdkValue::Int(n.min(i64::MAX as u64) as i64);
        refusal.detail = SdkValue::map([("needed", int(needed)), ("free", int(free))]);
        return Err(refusal);
    }
    Ok((source, target))
}

type Phased<T> = std::result::Result<T, (&'static str, SdkRefusal)>;

fn at<T>(phase: &'static str, result: Result<T>) -> Phased<T> {
    result.map_err(|refusal| (phase, refusal))
}

/// Enters `phase`: shown as progress (with `bytes` done and total while
/// sending) and written to the record's `op`.
fn enter(
    context: &mut Context<'_>,
    op: &mut Op,
    name: &str,
    phase: &'static str,
    bytes: Option<(u64, u64)>,
) -> Phased<()> {
    progress(context, phase, bytes);
    let home = context.home.clone();
    at(phase, advance(&home, name, op, phase))
}

fn phases(
    context: &mut Context<'_>,
    op: &mut Op,
    name: &str,
    (from, source): (&str, &mut Executor),
    (to, target): (&str, &mut Executor),
) -> Phased<SdkValue> {
    let home = context.home.clone();
    enter(context, op, name, "snapshot", None)?;
    let snapshot = at(
        "snapshot",
        step(&home, source, "snapshot", of_store(name, vec![])),
    )?;
    let total = snapshot.get_int("bytes").unwrap_or(0) as u64;
    // send, in pieces through this process
    let mut offset = 0u64;
    enter(context, op, name, "send", Some((0, total)))?;
    loop {
        let piece = at(
            "send",
            step(
                &home,
                source,
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
                target,
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
        progress(context, "send", Some((offset, total)));
        if len < CHUNK {
            break;
        }
    }
    // verify, install
    enter(context, op, name, "verify", None)?;
    let digest: Vec<(&str, SdkValue)> = ["bytes", "sha256", "heads"]
        .into_iter()
        .map(|field| (field, snapshot.get(field).cloned().unwrap_or_default()))
        .collect();
    at(
        "verify",
        step(&home, target, "verify", of_store(name, digest)),
    )?;
    // install puts the target's copy in place, which no step undoes with
    // certainty: from here until the record names the target a crash leaves
    // both copies, so the intent is written uncertain before it (H4–H6); the
    // next push or pull deletes the copy the record does not name
    *op = op.clone().as_uncertain();
    enter(context, op, name, "install", None)?;
    at(
        "install",
        step(&home, target, "install", of_store(name, vec![])),
    )?;
    // record: the Store is on `to` from here on. The same write records
    // that the source's copy is not yet retired and keeps `op`, certain, at
    // retire while it is: a retire that does not finish leaves a Store in
    // use on `to` and a copy the next push or pull deletes first
    enter(context, op, name, "record", None)?;
    let last = SdkValue::map([
        ("call", SdkValue::text(op.name())),
        ("from", SdkValue::text(from)),
        ("to", SdkValue::text(to)),
        ("at", SdkValue::Int(home::now_ms())),
        ("bytes", SdkValue::Int(total as i64)),
        (
            "sha256",
            snapshot.get("sha256").cloned().unwrap_or_default(),
        ),
        ("heads", snapshot.get("heads").cloned().unwrap_or_default()),
        ("retired", SdkValue::Bool(false)),
    ]);
    *op = op.clone().at("retire");
    let retiring = with_field(op.to_record().clone(), "uncertain", SdkValue::Bool(false));
    at(
        "record",
        (|| {
            let _store = record::lock(&home, name)?;
            let record = record_of(&home, name)?.with("machine", SdkValue::text(to));
            let record = record.with("last_transfer", last.clone());
            record.with(op::FIELD, retiring.clone()).write(&home)
        })(),
    )?;
    // retire the source's copy
    progress(context, "retire", None);
    at(
        "retire",
        step(&home, source, "retire", of_store(name, vec![])),
    )?;
    at("retire", retired(&home, name, op.id()))?;
    Ok(SdkValue::map([
        ("store", SdkValue::text(name)),
        ("from", SdkValue::text(from)),
        ("to", SdkValue::text(to)),
        ("bytes", SdkValue::Int(total as i64)),
        ("heads", snapshot.get("heads").cloned().unwrap_or_default()),
    ]))
}
