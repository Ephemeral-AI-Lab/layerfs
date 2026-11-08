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
    let transfer = |op: &Op| matches!(op.name(), "store.push" | "store.pull");
    match op.as_ref().map(|op| (op.standing(), transfer(op))) {
        Some((Standing::Running, _)) | Some((Standing::Unresolved, false)) => return None,
        _ => {}
    }
    if let Some(op) = op.filter(transfer) {
        let side = |field| op.args().get_text(field).map(str::to_owned);
        let (from, to) = (side("from")?, side("to")?);
        let on = match machine {
            _ if machine == to => Some(from.clone()),
            _ if machine == from && op.uncertain() => Some(to.clone()),
            _ => None,
        };
        if let Some(on) = on {
            let call = op.name().to_owned();
            return Some(Leftover { call, from, to, on });
        }
    }
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

/// Deletes Store `name`'s leftover copy, if its record names one (see
/// [`leftover`]): `native`'s file (with its write-ahead log and any partial
/// copy), or a machine's whole Store folder under its worker's root. A
/// stopped machine is `Unavailable` (#535 D11). A failure puts the ended
/// intent back, so the copy stays this call's to delete next time.
pub fn resolve(home: &Path, name: &str, related: Related) -> Result<()> {
    let record = record_of(home, name)?;
    let Some(left) = leftover(&record) else {
        return Ok(());
    };
    let held = record.op();
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
