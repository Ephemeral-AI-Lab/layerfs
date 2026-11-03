//! What an ended push or pull left (#536 §7.1, H4–H6): a copy of the Store
//! on the machine its record does not name. A gone process's uncertain
//! push or pull (it ended between install and record, or a step's answer
//! was lost) may have put the Store's copy on its target while the record
//! still names its source. The next `store push` or `store pull`, the call
//! such an intent's refusal names, deletes that copy before anything else,
//! under an intent of its own with the same call and sides, so a crash
//! meanwhile leaves the same copy to the call after it. The copy the record
//! names is never touched, and nothing is deleted while another operation
//! holds the Store.
use crate::common::op::{Op, Standing};
use crate::common::outcome::refused;
use crate::common::place::{self, Executor};
use crate::store::family::{begin, ended, of_store, record_of, replace_op, step};
use crate::store::record::StoreRecord;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use std::path::Path;

type Result<T> = std::result::Result<T, SdkRefusal>;

/// A Store copy its record does not name, left by an ended push or pull.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Leftover {
    /// The call that left it: `store.push` or `store.pull`.
    pub call: String,
    /// That call's source machine.
    pub from: String,
    /// That call's target machine.
    pub to: String,
    /// The machine that may hold the copy.
    pub on: String,
}

/// The copy `record` says an ended push or pull may have left: a gone
/// process's uncertain `store.push` or `store.pull` intent leaves one on
/// whichever of its two machines the record does not name.
pub(crate) fn leftover(record: &StoreRecord) -> Option<Leftover> {
    let machine = record.machine().ok()?;
    let op = record.op()?;
    if !matches!(op.name(), "store.push" | "store.pull") || op.standing() != Standing::Unresolved {
        return None;
    }
    let side = |field| op.args().get_text(field).map(str::to_owned);
    let (from, to) = (side("from")?, side("to")?);
    let on = match machine {
        _ if machine == from => to.clone(),
        _ if machine == to => from.clone(),
        _ => return None,
    };
    Some(Leftover {
        call: op.name().to_owned(),
        from,
        to,
        on,
    })
}

/// Deletes Store `name`'s leftover copy, if its record names one (see
/// [`leftover`]): `native`'s file (with its write-ahead log and any partial
/// copy), or a machine's whole Store folder under its worker's root. A
/// stopped machine is `Unavailable` (#535 D11). A failure puts the ended
/// intent back, so the copy stays this call's to delete next time.
pub(super) fn resolve(home: &Path, name: &str) -> Result<()> {
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
    begin(home, name, &op, |now| {
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
        Executor::Local => "retire",
        Executor::Worker(_) => "remove",
    };
    if let Err(refusal) = step(home, &mut executor, retire, of_store(name, vec![])) {
        ended(home, name, &op, "retire", &refusal, false, held.as_ref());
        return Err(refusal);
    }
    replace_op(home, name, op.id(), None)
}
