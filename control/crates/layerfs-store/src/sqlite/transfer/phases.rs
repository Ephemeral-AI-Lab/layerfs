//! `store.push` and `store.pull` (#536 §7.3, H1): a sqlite Store's file
//! between `native` and a machine, one phase at a time (check, snapshot,
//! send, verify, install, record, retire). A push sends it from `native`
//! to a machine's worker, a pull from its machine's worker to `native`:
//! the phases are the same, only the sides swap. Between two machines the
//! caller pulls, then pushes; each half is its own operation.
use crate::place::{self, Executor};
use crate::record::{self, StoreRecord};
use crate::sqlite::family::{
    advance, begin, ended, progress, record_of, settle, step_observed, step_on,
};
use crate::sqlite::transfer::resolve::{note_incoming, retired};
use crate::sqlite::transfer::step::{StepFrame, StoreStep};
use crate::sqlite::transfer::steps::CHUNK;
use crate::Related;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_home::events::Events;
use layerfs_home::home::{self, with_field};
use layerfs_home::op::{self, Op};
use layerfs_home::outcome::{refused, Outcome};
use std::path::Path;

type Result<T> = std::result::Result<T, SdkRefusal>;

/// `store.push`: the Store's file from `native` to machine `to`.
pub fn push(
    home: &Path,
    events: &mut Events<'_>,
    name: &str,
    to: &str,
    related: Related,
) -> Outcome {
    layerfs_home::names::MachineName::parse(to)?;
    transfer(home, events, name, Some(to), related)
}

/// `store.pull`: the Store's file from its machine to `native`.
pub fn pull(home: &Path, events: &mut Events<'_>, name: &str, related: Related) -> Outcome {
    transfer(home, events, name, None, related)
}

/// Where the Store goes from and to: a push (`to` named) from `native`, a
/// pull from the machine its record names.
fn sides(home: &Path, record: &StoreRecord, to: Option<&str>) -> Result<(String, String)> {
    match to {
        Some(to) => {
            super::resolve::can_push(home, record, to)?;
            Ok(("native".to_owned(), to.to_owned()))
        }
        None => Ok((super::resolve::can_pull(record)?, "native".to_owned())),
    }
}

fn transfer(
    home: &Path,
    events: &mut Events<'_>,
    name: &str,
    target: Option<&str>,
    related: Related,
) -> Outcome {
    let home = home.to_path_buf();
    let call = match target {
        Some(_) => "store.push",
        None => "store.pull",
    };
    let mut written = None;
    let result = run(&home, events, name, target, call, &mut written, related);
    if let (Err((phase, refusal)), Some(op)) = (&result, &written) {
        // past install both copies exist: a record phase that failed (its
        // lock wait expired, say) leaves the outcome open; a certain install
        // failure put nothing in place; past record the Store is on `to`,
        // and a retire that did not finish leaves `last_transfer` saying so
        let uncertain = *phase != "retire" && (refusal.code == "Unknown" || *phase == "record");
        // a push that failed certain at `send`, `verify` or `install` keeps
        // its target's partial copy and drops the intent that named that
        // machine: the record keeps the machine instead, so the doctor reports
        // the copy and `store.remove` deletes it there (§7.3, P6)
        if !uncertain && call == "store.push" && matches!(*phase, "send" | "verify" | "install") {
            if let Some(to) = op.args().get_text("to") {
                let _ = note_incoming(&home, name, to);
            }
        }
        ended(&home, name, op, phase, refusal, uncertain, None);
    }
    let verb = call.replace('.', " ");
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
    target: Option<&str>,
    call: &str,
    written: &mut Option<Op>,
    related: Related,
) -> Phased<SdkValue> {
    let home = home.to_path_buf();
    // check, before `op` and outside the locks: the Store's side, both
    // machines, the target's file and its free disk; the side again under
    // the locks, where `op` is written
    progress(events, "check", None);
    // first the copy an ended push or pull left, which this call clears
    at(
        "check",
        super::resolve::resolve(&home, name, call, target, related),
    )?;
    let record = at("check", record_of(&home, name))?;
    at("check", record.unblocked())?;
    let (from, to) = at("check", sides(&home, &record, target))?;
    // what holds the Store is refused before any machine is reached
    at("check", held_now(&home, name, related))?;
    let (mut source, mut target_side) = at("check", reach(&home, name, &from, &to))?;
    let args = SdkValue::map([("from", SdkValue::text(&from)), ("to", SdkValue::text(&to))]);
    let op = at("check", Op::begin(call, "check", args))?;
    let begun = begin(&home, name, &op, related, |record| {
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
    let _lease = at("check", settle(&home, name, related))?;
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
fn held_now(home: &Path, name: &str, related: Related) -> Result<()> {
    let _store = crate::record::lock(home, name)?;
    (related.store_idle)(home, name)
}

/// The headroom a target keeps beyond the Store's bytes: at least 1 GiB,
/// and a tenth of the Store when that is more.
pub const HEADROOM: u64 = 1 << 30;

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

/// How many times one piece may be asked for again, and how long to wait
/// before each: `send`'s read changes nothing (the source's Store is idle and
/// frozen under this call's lease, and a read at an offset is the same bytes
/// whenever it is taken), so a worker whose answer was lost is asked again
/// (#13); a step that writes is never replayed.
const READ_RETRIES: u32 = 3;
/// The pause before asking a worker for the same piece again.
const READ_PAUSE: std::time::Duration = std::time::Duration::from_millis(50);

/// Piece at `offset` of the Store `source` sends, read from `native` or a
/// machine's worker. A lost answer (`Unknown`) to a machine's read is asked
/// again up to [`READ_RETRIES`] times: it is the one step of a transfer that
/// no repeat can double-apply. Every other refusal is the call's.
fn read_piece(home: &Path, source: &mut Executor, name: &str, offset: u64) -> Result<SdkValue> {
    let mut retried = 0;
    loop {
        let step = StoreStep::Read {
            offset,
            len: CHUNK,
            staged: false,
        };
        match step_on(home, source, name, step) {
            Ok(piece) => return Ok(piece),
            Err(refusal)
                if retried < READ_RETRIES
                    && refusal.code == "Unknown"
                    && matches!(source, Executor::Worker(_)) =>
            {
                retried += 1;
                std::thread::sleep(READ_PAUSE);
            }
            // the reads changed nothing, so a lost answer is still the
            // call's refusal: say how many times the piece was asked
            Err(mut refusal) => {
                if retried > 0 {
                    refusal.message = format!(
                        "{} (the piece was asked {} more times, and every answer was lost)",
                        refusal.message, retried
                    );
                    refusal.detail =
                        with_field(refusal.detail, "retries", SdkValue::Int(retried as i64));
                }
                return Err(refusal);
            }
        }
    }
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
    let store_record = at("snapshot", record_of(&home, name))?.to_record().clone();
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
        let piece = at("send", read_piece(&home, source, name, offset))?;
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
    // next push or pull deletes the copy the record does not name
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
    // use on `to` and a copy the next push or pull deletes first
    enter(&home, events, op, name, "record", None)?;
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
    // retire the source's copy: `native`'s file (its folder holds the
    // home's records), or the Store's whole folder under a worker's root,
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
        ("store", SdkValue::text(name)),
        ("from", SdkValue::text(from)),
        ("to", SdkValue::text(to)),
        ("bytes", SdkValue::Int(total as i64)),
        ("heads", snapshot.get("heads").cloned().unwrap_or_default()),
    ]))
}
