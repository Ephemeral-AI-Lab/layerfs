//! What an ended push or pull left (#536 §7.1, §7.3, H4–H6): a copy of
//! the Store on the machine its record does not name. A gone process's
//! uncertain push or pull (it ended during install or record, or a step's
//! answer was lost) may have put the Store's copy on its target while the
//! record still names its source; a push or pull whose retire did not
//! finish (it failed, or its process ended) leaves the source's copy, and
//! its `last_transfer` says `retired: false`. The next `store push` or
//! `store pull`, the call an unresolved intent's refusal and the doctor
//! name, deletes that copy before anything else, under an intent of its
//! own with the same call and sides, so a crash meanwhile leaves the same
//! copy to the call after it. The copy the record names is never touched,
//! and nothing is deleted while another operation holds the Store.
//!
//! A `store.push` that failed certain at `send`, `verify` or `install` is the
//! one case with no intent left to read: it keeps its target's partial copy
//! (`store.sqlite.incoming`) and drops the `op` that named the machine, so
//! the record keeps the machine itself (`incoming_on`, [`note_incoming`]).
//! The doctor reports that copy's bytes through [`partial_on`], and the next
//! `store push` or `store remove` deletes it there: until its record names the
//! target the Store is on `native`, where `can_pull` refuses, so a pull is
//! never what clears it.
use crate::place::{self, Executor};
use crate::record::{self, StoreRecord, StoreRef};
use crate::sqlite::family::{begin, ended, record_of, step_on};
use crate::sqlite::transfer::step::StoreStep;
use crate::Related;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_home::op::{Op, Standing};
use layerfs_home::outcome::refused;
use layerfs_machine::record::MachineRecord;
use std::path::Path;

type Result<T> = std::result::Result<T, SdkRefusal>;

/// A Store copy its record does not name, left by an ended push or pull.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Leftover {
    /// The call that left it: `store.push` or `store.pull`.
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
/// process's `store.push` or `store.pull` intent that is uncertain or
/// whose record write was made, else a `last_transfer` not yet retired.
/// None while another operation runs, or for a gone process's unresolved
/// intent of another call (its own clearing call clears it).
pub fn leftover(record: &StoreRecord) -> Option<Leftover> {
    let machine = record.machine().ok()?;
    let op = record.op();
    // an interrupted `store.remove` names the copy it had not cleaned yet
    if let Some(op) = op
        .as_ref()
        .filter(|op| op.standing() == Standing::Unresolved && op.name() == "store.remove")
    {
        let on = op.args().get_text("leftover")?.to_owned();
        if on != machine {
            return Some(Leftover {
                call: "store.remove".to_owned(),
                from: on.clone(),
                to: machine.to_owned(),
                on,
            });
        }
        return None;
    }
    let transfer = |op: &Op| matches!(op.name(), "store.push" | "store.pull");
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
        let staged = op.uncertain()
            || (op.name() == "store.push" && matches!(op.phase(), "send" | "verify"));
        let on = match machine {
            _ if machine == to => Some(from.clone()),
            _ if machine == from && staged => Some(to.clone()),
            _ => None,
        };
        if let Some(on) = on {
            let call = op.name().to_owned();
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
    let (call, from, to) = (side("call")?, side("from")?, side("to")?);
    (machine == to).then(|| Leftover {
        call,
        on: from.clone(),
        from,
        to,
    })
}

/// A certain `store.push` failure at `send`, `verify` or `install` left a
/// partial copy on its target and dropped the intent that named it
/// ([`note_incoming`] kept the machine in the record).
fn partial(record: &StoreRecord, machine: &str) -> Option<Leftover> {
    let on = partial_on(record)?;
    (on != machine).then(|| Leftover {
        call: "store.push".to_owned(),
        from: NATIVE.to_owned(),
        to: on.clone(),
        on,
    })
}

/// The machine a `store.push` may have left a partial copy
/// (`store.sqlite.incoming`) on: its target, read from the push's intent while
/// the record holds one (from `send` on the target has a piece; `install` and
/// later leave a whole file, which `stat` reports no partial copy of), else
/// from `incoming_on`, the machine a certain failure kept.
pub fn partial_on(record: &StoreRecord) -> Option<String> {
    let sent = record
        .op()
        .filter(|op| op.name() == "store.push")
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

/// Records that a certain `store.push` failure left a partial copy on `to`:
/// the intent that named that machine is gone, so the record keeps it for the
/// doctor and for `store.remove`, and the next `store push` or `store remove`
/// clears it (see [`leftover`]).
pub fn note_incoming(home: &Path, name: &str, to: &str) -> Result<()> {
    let _store = record::lock(home, name)?;
    record_of(home, name)?
        .with("incoming_on", SdkValue::text(to))
        .write(home)
}

/// Deletes Store `name`'s leftover copy, if its record names one (see
/// [`leftover`]): `native`'s file (with its write-ahead log and any partial
/// copy), or a machine's whole Store folder under its worker's root.
/// An unresolved intent is cleared only by the call its refusal names in the
/// same direction it named (a `store.push` to the same `to`, or a
/// `store.pull`); a wrong direction or target is refused `Unresolved`
/// without deleting either copy. A stopped machine is `Unavailable` (#535
/// D11). A failure puts the ended intent back, so the copy stays this call's
/// to delete next time.
pub fn resolve(
    home: &Path,
    name: &str,
    call: &str,
    target: Option<&str>,
    related: Related,
) -> Result<()> {
    let record = record_of(home, name)?;
    let held = record.op();
    if let Some(op) = held
        .as_ref()
        .filter(|op| op.standing() == Standing::Unresolved)
    {
        let same_direction = match (call, target, op.name(), op.args().get_text("to")) {
            ("store.push", Some(want), "store.push", Some(to)) => want == to,
            ("store.pull", None, "store.pull", _) => true,
            _ => false,
        };
        if !same_direction {
            return record.unblocked();
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
    let mut op = Op::begin(&left.call, "retire", args)?;
    if held.as_ref().is_some_and(Op::uncertain) {
        op = op.as_uncertain();
    }
    let id = held.as_ref().map(|op| op.id().to_owned());
    begin(home, name, &op, related, |now| {
        if now.op().map(|op| op.id().to_owned()) != id || leftover(now).as_ref() != Some(&left) {
            now.unblocked()?;
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
    let mut now = record_of(home, name)?;
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

/// The machine that is this host.
const NATIVE: &str = "native";

/// `store.push`: refused `InvalidInput` unless `record` is a sqlite Store
/// on `native` and `to` another machine; `NotFound` for an unknown `to`.
/// Its in-use half, `Related::store_idle`, runs under the Store's lock.
pub fn can_push(home: &Path, record: &StoreRecord, to: &str) -> Result<()> {
    let name = record.name();
    let on = sqlite_machine(record)?;
    if on != NATIVE {
        return Err(refused(
            "InvalidInput",
            format!("Store {name} is on machine {on}; `layerfs store pull {name}` brings it to {NATIVE} first"),
        ));
    }
    if to == NATIVE {
        return Err(refused(
            "InvalidInput",
            format!("Store {name} is already on {NATIVE}"),
        ));
    }
    if MachineRecord::read(home, to)?.is_none() {
        return Err(refused("NotFound", format!("no machine named {to}")));
    }
    Ok(())
}

/// `store.pull`: refused `InvalidInput` unless `record` is a sqlite Store
/// on a machine other than `native`; answers that machine. Its in-use half,
/// `Related::store_idle`, runs under the Store's lock.
pub fn can_pull(record: &StoreRecord) -> Result<String> {
    let on = sqlite_machine(record)?;
    if on == NATIVE {
        return Err(refused(
            "InvalidInput",
            format!("Store {} is already on {NATIVE}", record.name()),
        ));
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
