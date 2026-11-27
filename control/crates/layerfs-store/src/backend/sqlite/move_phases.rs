//! `store.move` (#536 §7.3, H1; call surface 2.0): a sqlite Store's file
//! between the machine whose calls run in this process (`native` on macOS
//! and Linux) and another machine, one phase at a time (check, snapshot,
//! send, verify, install, record, retire). One end is always that machine
//! (C13): to a machine's worker from it, or from its machine's worker to
//! it; the phases are the same, only the sides swap. Between two other
//! machines it is refused, naming the move to that machine that comes
//! first; a host with no such machine refuses every move.
use crate::admin::host_worker::handoff::{self, Worker};
use crate::admin::record::intent;
use crate::admin::record::store::{self, StoreRecord};
use crate::admin::route::executor::{self, Executor};
use crate::backend::sqlite::family::{
    advance, begin_maintenance, ended, progress, settle, step_observed, step_on,
};
use crate::backend::sqlite::move_leftover::{note_incoming, retired};
use crate::backend::sqlite::step_request::{StepFrame, StoreStep};
use crate::backend::sqlite::step_run::CHUNK;
use crate::Related;
use layerfs_home::events::Events;
use layerfs_home::home::{self, with_field};
use layerfs_home::op::{self, Op};
use layerfs_home::outcome::shell_word;
use layerfs_home::outcome::{refused, with_fix, Outcome};
use layerfs_protocol::sdk::{SdkRefusal, SdkValue};
use std::path::Path;

type Result<T> = std::result::Result<T, SdkRefusal>;

/// `store.move`: the Store's file from the machine that holds it to `to`.
pub fn move_to(
    home: &Path,
    events: &mut Events<'_>,
    name: &str,
    to: &str,
    related: Related,
) -> Outcome {
    layerfs_home::names::MachineName::parse(to)?;
    transfer(home, events, name, to, related)
}

/// Where the Store goes from and to: from the machine its record names to
/// `to`, one of the two being this computer.
fn sides(home: &Path, record: &StoreRecord, to: &str) -> Result<(String, String)> {
    Ok((
        super::move_leftover::can_move(home, record, to)?,
        to.to_owned(),
    ))
}

fn transfer(
    home: &Path,
    events: &mut Events<'_>,
    name: &str,
    target: &str,
    related: Related,
) -> Outcome {
    let home = home.to_path_buf();
    let call = intent::MOVE;
    let mut written = None;
    let result = run(&home, events, name, target, call, &mut written, related);
    if let (Err((phase, refusal)), Some(op)) = (&result, &written) {
        // past install both copies exist: a record phase that failed (its
        // lock wait expired, say) leaves the outcome open; a certain install
        // failure put nothing in place; past record the Store is on `to`,
        // and a retire that did not finish leaves `last_transfer` saying so
        let uncertain = *phase != "retire" && (refusal.code == "Unknown" || *phase == "record");
        // a move to a machine that failed certain at `send`, `verify` or
        // `install` keeps its target's partial copy and drops the intent that
        // named that machine: the record keeps the machine instead, so the
        // doctor reports the copy and `store.remove` deletes it there (§7.3, P6)
        if !uncertain && matches!(*phase, "send" | "verify" | "install") {
            let here = layerfs_machine::in_process();
            if let Some(to) = op.args().get_text("to").filter(|to| Some(*to) != here) {
                let _ = note_incoming(&home, name, to);
            }
        }
        ended(&home, name, op, phase, refusal, uncertain, None);
    }
    let verb = "store move";
    result.map_err(|(phase, mut refusal)| {
        // past record the Store is on `to`: a retire that did not finish
        // says which machine now holds it and which copy stayed
        if phase == "retire" {
            if let Some(op) = &written {
                let from = op.args().get_text("from").unwrap_or("");
                let to = op.args().get_text("to").unwrap_or("");
                refusal.message = format!(
                    "{verb}, {phase}: Store {name} is now on {to} (authority switched to {to}; \
                     source copy on {from} was not retired: {})",
                    refusal.message
                );
                refusal.detail = with_field(refusal.detail, "authority", SdkValue::text(to));
                refusal.detail = with_field(refusal.detail, "source", SdkValue::text(from));
                refusal.detail = with_field(refusal.detail, "retired", SdkValue::Bool(false));
                refusal.detail = with_field(refusal.detail, "phase", SdkValue::text(phase));
                return refusal;
            }
        }
        refusal.message = format!("{verb}, {phase}: {}", refusal.message);
        refusal.detail = with_field(refusal.detail, "phase", SdkValue::text(phase));
        refusal
    })
}

/// The check phase, then the rest once `op` is written into `written`.
fn run(
    home: &Path,
    events: &mut Events<'_>,
    name: &str,
    target: &str,
    call: &str,
    written: &mut Option<Op>,
    related: Related,
) -> Phased<SdkValue> {
    let home = home.to_path_buf();
    // check, before `op` and outside the locks: the Store's side, both
    // machines, the target's file and its free disk; the side again under
    // the locks, where `op` is written
    progress(events, "check", None);
    at("check", handoff::recover(&home, name, call, Some(target)))?;
    // first the copy an ended move left, which this call clears
    at(
        "check",
        super::move_leftover::resolve(&home, name, target, related),
    )?;
    let record = at("check", StoreRecord::of(&home, name))?;
    at("check", record.unheld())?;
    let (from, to) = at("check", sides(&home, &record, target))?;
    // what holds the Store is refused before any machine is reached
    let worker = handoff::observe(&home, &record);
    at("check", held_now(&home, name, related, worker.as_ref()))?;
    let (mut source, mut target_side) = at("check", reach(&home, name, &from, &to))?;
    let args = SdkValue::map([("from", SdkValue::text(&from)), ("to", SdkValue::text(&to))]);
    let op = at("check", Op::begin(call, "check", args))?;
    let begun = begin_maintenance(&home, name, &op, related, worker, |record| {
        record.unheld()?;
        if sides(&home, record, target)? != (from.clone(), to.clone()) {
            return Err(refused(
                "Conflict",
                format!("Store {name} moved meanwhile; nothing was sent"),
            ));
        }
        Ok(())
    });
    let ((), worker) = at("check", begun)?;
    at(
        intent::WORKER_CLOSE,
        handoff::finish(&home, name, &op, worker.as_ref(), Some(related), None),
    )?;
    let op = written.insert(op);
    let _lease = at("check", settle(&home, name, related, op))?;
    phases(
        &home,
        events,
        op,
        name,
        (&from, &mut source),
        (&to, &mut target_side),
    )
}

/// Refuses `InUse` while leases or Sandbox records hold Store `name`,
/// read under the Store's lock (stale leases are dropped there). The
/// check is repeated where `op` is written.
fn held_now(home: &Path, name: &str, related: Related, worker: Option<&Worker>) -> Result<()> {
    let _home = layerfs_home::home::lock::take(home)?;
    let _store = crate::admin::record::store::lock(home, name)?;
    let record = StoreRecord::of(home, name)?;
    related.check_idle(home, name, Worker::holder(worker, home, name, &record)?)
}

/// The headroom a target keeps beyond the Store's bytes: at least 1 GiB,
/// and a tenth of the Store when that is more.
pub const HEADROOM: u64 = 1 << 30;

/// Both machines' executors, once each answers: the target holds no Store
/// file named `name` and has the Store's bytes (with its write-ahead log)
/// plus [`HEADROOM`] free (#536 §7.3, §11 item 7). A stopped machine is
/// `Unavailable`, with `layerfs machine start` as its fix (#535 D11).
fn reach(home: &Path, name: &str, from: &str, to: &str) -> Result<(Executor, Executor)> {
    // the Store is blocked by a source machine with no vm_name (LF10)
    let mut source =
        executor::on_machine(home, from).map_err(|why| executor::blocked(name, from, why))?;
    let mut target = executor::on_machine(home, to)?;
    for (machine, executor) in [(from, &mut source), (to, &mut target)] {
        if let Executor::Worker(worker) = executor {
            let start = format!(
                "layerfs machine start {machine}",
                machine = shell_word(machine)
            );
            worker.ready().map_err(|refusal| with_fix(refusal, start))?;
        }
    }
    let held = step_on(home, &mut source, name, StoreStep::Stat)?;
    let there = step_on(home, &mut target, name, StoreStep::Stat)?;
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
            "Failed",
            format!("machine {to}'s worker answered no free disk"),
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
    home: &Path,
    events: &mut Events<'_>,
    op: &mut Op,
    name: &str,
    phase: &'static str,
    bytes: Option<(u64, u64)>,
) -> Phased<()> {
    progress(events, phase, bytes);
    let home = home.to_path_buf();
    at(phase, advance(&home, name, op, phase))
}

/// Completes the incomplete project records of Store `name` from the Store
/// file on this kernel (#749), before a worker's copy and the entries it
/// keeps are retired. Best effort: a record left incomplete does not fail
/// the move, which has installed the Store already; each is a line of
/// `log/repair.jsonl`, naming the project (none when the Store's records
/// were not read at all) and why.
fn complete_records(home: &Path, name: &str, call: &str) {
    let left = match StoreRecord::of(home, name) {
        Ok(record) => crate::data::project::record::restored::complete_existing(
            home,
            name,
            &record,
            &home::paths::store_file(home, name),
        ),
        Err(refusal) => vec![(String::new(), refusal.message)],
    };
    for (project, why) in left {
        let _ = home::log_line(
            home,
            "repair.jsonl",
            &SdkValue::map([
                ("at", SdkValue::Int(home::now_ms())),
                ("call", SdkValue::text(call)),
                ("store", SdkValue::text(name)),
                (
                    "project",
                    if project.is_empty() {
                        SdkValue::Null
                    } else {
                        SdkValue::text(project)
                    },
                ),
                ("message", SdkValue::text(why)),
            ]),
        );
    }
}

fn phases(
    home: &Path,
    events: &mut Events<'_>,
    op: &mut Op,
    name: &str,
    (from, source): (&str, &mut Executor),
    (to, target): (&str, &mut Executor),
) -> Phased<SdkValue> {
    let home = home.to_path_buf();
    // the Store's record travels with the snapshot: a crashed opener's
    // leftover write-ahead log is checkpointed through its binding
    let store_record = at("snapshot", StoreRecord::of(&home, name))?
        .to_record()
        .clone();
    enter(&home, events, op, name, "snapshot", None)?;
    let snapshot = at(
        "snapshot",
        step_observed(
            &home,
            events,
            "snapshot",
            source,
            &StepFrame {
                store: name.to_owned(),
                store_record: Some(store_record),
                step: StoreStep::Snapshot,
            },
        ),
    )?;
    let total = snapshot.get_int("bytes").unwrap_or(0) as u64;
    // send, in pieces through this process
    let mut offset = 0u64;
    enter(&home, events, op, name, "send", Some((0, total)))?;
    loop {
        let piece = at(
            "send",
            step_on(
                &home,
                source,
                name,
                StoreStep::Read {
                    offset,
                    len: CHUNK,
                    staged: false,
                },
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
            step_on(&home, target, name, StoreStep::Write { offset, bytes }),
        )?;
        offset += len;
        progress(events, "send", Some((offset, total)));
        if len < CHUNK {
            break;
        }
    }
    // verify, install
    enter(&home, events, op, name, "verify", None)?;
    let text = |field| snapshot.get_text(field).map(str::to_owned);
    let digest = StoreStep::Verify {
        bytes: snapshot.get_int("bytes"),
        sha256: text("sha256"),
        heads: text("heads"),
    };
    at(
        "verify",
        step_observed(
            &home,
            events,
            "verify",
            target,
            &StepFrame::on(name, digest),
        ),
    )?;
    // install puts the target's copy in place, which no step undoes with
    // certainty: from here until the record names the target a crash leaves
    // both copies, so the intent is written uncertain before it (H4–H6); the
    // next move deletes the copy the record does not name
    *op = op.clone().as_uncertain();
    enter(&home, events, op, name, "install", None)?;
    at(
        "install",
        step_observed(
            &home,
            events,
            "install",
            target,
            &StepFrame::on(name, StoreStep::Install),
        ),
    )?;
    // record: the Store is on `to` from here on. The same write records
    // that the source's copy is not yet retired and keeps `op`, certain, at
    // retire while it is: a retire that does not finish leaves a Store in
    // use on `to` and a copy the next move deletes first
    enter(&home, events, op, name, "record", None)?;
    if matches!(source, Executor::Local) || matches!(target, Executor::Local) {
        complete_records(&home, name, op.name());
    }
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
            let _store = store::lock(&home, name)?;
            let record = StoreRecord::of(&home, name)?.with("machine", SdkValue::text(to));
            let record = record.with("last_transfer", last.clone());
            record.with(op::FIELD, retiring.clone()).write(&home)
        })(),
    )?;
    // retire the source's copy: the file in this process's home, on a
    // machine whose calls run here (its folder holds the home's records), or the Store's whole folder under a worker's root,
    // with the project entries a worker keeps there
    progress(events, "retire", None);
    let retire = match source {
        Executor::Local => StoreStep::Retire,
        Executor::Worker(_) => StoreStep::Remove,
    };
    at(
        "retire",
        step_observed(
            &home,
            events,
            "retire",
            source,
            &StepFrame::on(name, retire),
        ),
    )?;
    at("retire", retired(&home, name, op.id()))?;
    Ok(SdkValue::map([
        ("moved_bytes", SdkValue::Int(total as i64)),
        ("outcome", SdkValue::text("moved")),
        ("store", SdkValue::text(name)),
        ("from", SdkValue::text(from)),
        ("to", SdkValue::text(to)),
        (
            "heads_sha256",
            snapshot.get("heads").cloned().unwrap_or_default(),
        ),
    ]))
}
