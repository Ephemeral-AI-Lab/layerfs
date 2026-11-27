//! What an ended move left (#536 §7.1, §7.3, H4–H6): a copy of the Store
//! on the machine its record does not name. A gone process's uncertain
//! move (it ended during install or record, or a step's answer was lost)
//! may have put the Store's copy on its target while the record still
//! names its source; a move whose retire did not finish (it failed, or its
//! process ended) leaves the source's copy, and its `last_transfer` says
//! `retired: false`. The next `store move`, the call an unresolved
//! intent's refusal and the doctor name, deletes that copy before anything
//! else, under an intent of its own with the same sides, so a crash
//! meanwhile leaves the same copy to the call after it. The copy the
//! record names is never touched, and nothing is deleted while another
//! operation holds the Store.
//!
//! A move to a machine that failed certain at `send`, `verify` or `install`
//! is the one case with no intent left to read: it keeps its target's
//! partial copy (`store.sqlite.incoming`) and drops the `op` that named
//! the machine, so the record keeps the machine itself (`incoming_on`,
//! [`note_incoming`]). The doctor reports that copy's bytes through
//! [`partial_on`], and the next `store move` or `store remove` deletes it
//! there.

use crate::intent;
use crate::place::{self, Executor};
use crate::record::{self, StoreRecord, StoreRef};
use crate::sqlite::family::{begin, ended, step_on};
use crate::sqlite::transfer::step::StoreStep;
use crate::Related;
use layerfs_protocol::provision::install::{SdkRefusal, SdkValue};
use layerfs_home::op::{Op, Standing};
use layerfs_home::outcome::shell_word;
use layerfs_home::outcome::{refused, with_fix};
use layerfs_machine::record::MachineRecord;
use std::path::Path;

type Result<T> = std::result::Result<T, SdkRefusal>;

/// A Store copy its record does not name, left by an ended move.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Leftover {
    /// The call that left it: `store.move`, or `store.remove` for an
    /// interrupted remove.
    pub call: String,
    /// That call's source machine.
    pub from: String,
    /// That call's target machine.
    pub to: String,
    /// The machine that may hold the copy.
    pub on: String,
}

/// The copy `record` says an ended push or pull may have left, on
/// whichever of its two machines the record does not name: a gone
/// process's move intent that is uncertain or
/// whose record write was made, else a `last_transfer` not yet retired.
/// None while another operation runs, or for a gone process's unresolved
/// intent of another call (its own clearing call clears it).
pub fn leftover(record: &StoreRecord) -> Option<Leftover> {
    let machine = record.machine().ok()?;
    let op = record.op();
    // an interrupted `store.remove` names the copy it had not cleaned yet
    if let Some(op) = op
        .as_ref()
        .filter(|op| op.standing() == Standing::Unresolved && op.name() == intent::REMOVE)
    {
        let on = op.args().get_text("leftover")?.to_owned();
        if on != machine {
            return Some(Leftover {
                call: intent::REMOVE.to_owned(),
                from: on.clone(),
                to: machine.to_owned(),
                on,
            });
        }
        return None;
    }
    let transfer = |op: &Op| intent::is_move(op.name());
    match op.as_ref().map(|op| (op.standing(), transfer(op))) {
        Some((Standing::Running, _)) | Some((Standing::Unresolved, false)) => return None,
        _ => {}
    }
    if let Some(op) = op.filter(transfer) {
        let side = |field| op.args().get_text(field).map(str::to_owned);
        let (from, to) = (side("from")?, side("to")?);
        // the target may hold the Store's copy once the intent is uncertain
        // (install and record are marked so), and once a push's `send` has
        // written its first piece there: a push whose process died certain at
        // `send` or `verify` left that partial copy with nothing else naming
        // its machine
        let push = here() == Some(from.as_str());
        let staged = op.uncertain() || (push && matches!(op.phase(), "send" | "verify"));
        let on = match machine {
            _ if machine == to => Some(from.clone()),
            _ if machine == from && staged => Some(to.clone()),
            _ => None,
        };
        if let Some(on) = on {
            let call = intent::MOVE.to_owned();
            return Some(Leftover { call, from, to, on });
        }
    }
    unretired(record, &machine).or_else(|| partial(record, &machine))
}

/// A `last_transfer` that did not retire its source's copy: the record names
/// the Store's new machine, and the copy is on the other side.
fn unretired(record: &StoreRecord, machine: &str) -> Option<Leftover> {
    let last = record.field("last_transfer")?;
    if last.get("retired") != Some(&SdkValue::Bool(false)) {
        return None;
    }
    let side = |field| last.get_text(field).map(str::to_owned);
    let (from, to) = (side("from")?, side("to")?);
    (machine == to).then(|| Leftover {
        call: intent::MOVE.to_owned(),
        on: from.clone(),
        from,
        to,
    })
}

/// A move to a machine that failed certain at `send`, `verify` or `install` left a
/// partial copy on its target and dropped the intent that named it
/// ([`note_incoming`] kept the machine in the record).
fn partial(record: &StoreRecord, machine: &str) -> Option<Leftover> {
    let (on, from) = (partial_on(record)?, here()?);
    (on != machine).then(|| Leftover {
        call: intent::MOVE.to_owned(),
        from: from.to_owned(),
        to: on.clone(),
        on,
    })
}

/// The machine a move from the machine whose calls run in this process
/// ([`here`]) may have left a partial copy
/// (`store.sqlite.incoming`) on: its target, read from the move's intent while
/// the record holds one (from `send` on the target has a piece; `install` and
/// later leave a whole file, which `stat` reports no partial copy of), else
/// from `incoming_on`, the machine a certain failure kept.
pub fn partial_on(record: &StoreRecord) -> Option<String> {
    let sent = record
        .op()
        .filter(|op| intent::is_move(op.name()))
        .filter(|op| here().is_some_and(|here| op.args().get_text("from") == Some(here)))
        .filter(|op| !matches!(op.phase(), "check" | "snapshot"))
        .and_then(|op| op.args().get_text("to").map(str::to_owned));
    if sent.is_some() {
        return sent;
    }
    match record.field("incoming_on") {
        Some(SdkValue::Text(machine)) => Some(machine.clone()),
        _ => None,
    }
}

/// Records that a certain failure of a move to `to` left a partial copy
/// there: the intent that named that machine is gone, so the record keeps it
/// for the doctor and for `store.remove`, and the next `store move` or
/// `store remove` clears it (see [`leftover`]).
pub fn note_incoming(home: &Path, name: &str, to: &str) -> Result<()> {
    let _store = record::lock(home, name)?;
    StoreRecord::of(home, name)?
        .with("incoming_on", SdkValue::text(to))
        .write(home)
}

/// Deletes Store `name`'s leftover copy, if its record names one (see
/// [`leftover`]): the file in this process's home, for a machine whose
/// calls run here (with its write-ahead log and any partial copy), or a
/// machine's whole Store folder under its worker's root.
/// An unresolved move is cleared only by the move its refusal names, to
/// the same machine `to`; a move to another machine is refused `Unresolved`
/// without deleting either copy, and so is a move while another call's
/// unresolved operation holds the Store. A stopped machine is `Unavailable`
/// (#535 D11). A failure puts the ended intent back, so the copy stays this
/// call's to delete next time.
pub fn resolve(home: &Path, name: &str, to: &str, related: Related) -> Result<()> {
    let record = StoreRecord::of(home, name)?;
    let held = record.op();
    if let Some(op) = held
        .as_ref()
        .filter(|op| op.standing() == Standing::Unresolved)
    {
        if !intent::is_move(op.name()) || op.args().get_text("to") != Some(to) {
            return record.unheld();
        }
    }
    let Some(left) = leftover(&record) else {
        return Ok(());
    };
    let mut executor = place::on_machine(home, &left.on)?;
    if let Executor::Worker(worker) = &mut executor {
        worker.ready()?;
    }
    let args = SdkValue::map([
        ("from", SdkValue::text(&left.from)),
        ("to", SdkValue::text(&left.to)),
    ]);
    let mut op = Op::begin(intent::MOVE, "retire", args)?;
    if held.as_ref().is_some_and(Op::uncertain) {
        op = op.as_uncertain();
    }
    let id = held.as_ref().map(|op| op.id().to_owned());
    begin(home, name, &op, related, |now| {
        if now.op().map(|op| op.id().to_owned()) != id || leftover(now).as_ref() != Some(&left) {
            now.unheld()?;
            return Err(refused(
                "Conflict",
                format!("Store {name} changed meanwhile; nothing was deleted"),
            ));
        }
        Ok(())
    })?;
    let retire = match executor {
        Executor::Local => StoreStep::Retire,
        Executor::Worker(_) => StoreStep::Remove,
    };
    if let Err(refusal) = step_on(home, &mut executor, name, retire) {
        ended(home, name, &op, "retire", &refusal, false, held.as_ref());
        return Err(refusal);
    }
    retired(home, name, op.id())
}

/// Under the Store's lock: drops `op` while it is still the operation `id`,
/// and marks a `last_transfer` not yet retired as retired.
pub fn retired(home: &Path, name: &str, id: &str) -> Result<()> {
    let _store = record::lock(home, name)?;
    let mut now = StoreRecord::of(home, name)?;
    if now.op().is_some_and(|held| held.id() == id) {
        now = now.without(layerfs_home::op::FIELD);
    }
    // the copy this call cleared or replaced is no longer on that machine
    if now.field("incoming_on").is_some() {
        now = now.without("incoming_on");
    }
    if let Some(last) = now.field("last_transfer").cloned() {
        if last.get("retired") == Some(&SdkValue::Bool(false)) {
            let last = layerfs_home::home::with_field(last, "retired", SdkValue::Bool(true));
            now = now.with("last_transfer", last);
        }
    }
    now.write(home)
}

/// The machine whose calls run in this process (`native` on macOS and
/// Linux), one end of every move (C13); none on a host where every
/// machine has a worker.
fn here() -> Option<&'static str> {
    layerfs_machine::in_process()
}

/// `store.move`: the machine sqlite Store `record` is on, from which it
/// goes to `to`. Refused `InvalidInput` when it is on `to` already, or
/// when neither is the machine whose calls run in this process (C13: a
/// Store moves through this process, so one end is that machine), with
/// the move to that machine as its fix; `Unsupported` on a host with no
/// such machine; `NotFound` for an unknown `to`. Its in-use half,
/// `Related::store_idle`, runs under the Store's lock.
pub fn can_move(home: &Path, record: &StoreRecord, to: &str) -> Result<String> {
    let name = record.name();
    let on = sqlite_machine(record)?;
    if on == to {
        return Err(refused(
            "InvalidInput",
            format!("Store {name} is already on {to}"),
        ));
    }
    let here = here().ok_or_else(|| layerfs_machine::none_in_process("store.move"))?;
    if on != here && to != here {
        return Err(with_fix(
            refused(
                "InvalidInput",
                format!(
                    "Store {name} is on machine {on}: a Store moves only to or from machine \
                     {here}, whose calls run in this process; `layerfs store move {name} {here}` \
                     brings it there first"
                ),
            ),
            format!("layerfs store move {name} {here}", name = shell_word(name)),
        ));
    }
    if to != here && MachineRecord::read(home, to)?.is_none() {
        return Err(refused("NotFound", format!("no machine named {to}")));
    }
    Ok(on)
}

/// The machine of sqlite Store `record`; a PostgreSQL Store never moves.
fn sqlite_machine(record: &StoreRecord) -> Result<String> {
    if let StoreRef::Postgres { .. } = record.store_ref()? {
        return Err(refused(
            "InvalidInput",
            format!(
                "Store {} is a PostgreSQL database; it does not move",
                record.name()
            ),
        ));
    }
    record.machine()
}
