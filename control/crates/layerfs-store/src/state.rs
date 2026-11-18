//! A Store's state from the home's records alone (call surface 2.0): one
//! read of its record and its machine's, no probe. [`recorded`] is the one
//! derivation: the project package (a project of a Store that is not ready
//! is `blocked`), the home tree, the Store doctor and the Sandbox package
//! use it, and `store.list` and `store.get` answer it.
//!
//! In order: the operation the record holds (`busy` while its owner lives,
//! `broken` `unresolved` once a gone owner left its outcome unknown; a gone
//! owner's certain operation is ignored, with no write); a sqlite Store's
//! machine that is not ready (`blocked`, naming the machine); what the
//! last live read found and wrote ([`SEEN`]: `missing`, `unreachable`, or
//! an unsettled compaction only the Store's own machine records); else
//! `ready`. `unknown` is never derived here: only a live read that could
//! not be made answers it (`live`).
use crate::intent;
use crate::record::{Placement, StoreRecord, StoreRef};
use layerfs_bridge::control::SdkRefusal;
use layerfs_home::op::Standing;
use layerfs_home::state::{Reason, State};
use std::path::Path;

type Result<T> = std::result::Result<T, SdkRefusal>;

/// The record field that holds what the last live read found wrong with
/// the Store, as a reason word: `missing`, `unreachable` or `unresolved`.
/// Absent while the Store was found well, or never read live.
pub const SEEN: &str = "broken";

/// How a machine's records-only state is derived, handed in by the caller.
///
/// **The seam with the machine family.** A Store on a machine is `blocked`
/// by what that machine's state says. The one derivation of it is the
/// inventory's `machine_state` in the control crate (over
/// `layerfs_machine::state`, adding the removed-platform check), which
/// this crate sits below; so every function here that needs a machine's
/// state takes this, and the calls pass the control crate's
/// `store::machine_state`.
pub type Machines<'a> = &'a dyn Fn(&Path, &str) -> Result<State>;

/// The state the record's operation gives the Store, if it holds one that
/// still means something: `busy` with the operation, or `broken`
/// `unresolved` with the call that clears it as its fix.
pub fn held(record: &StoreRecord) -> Option<State> {
    let op = record.op()?;
    match op.standing() {
        Standing::Running => Some(State::Busy {
            name: intent::word(op.name()).to_owned(),
            phase: op.phase().to_owned(),
            started_ms: op.to_record().get_int("started"),
        }),
        Standing::Unresolved => Some(State::broken(
            Reason::Unresolved,
            Some(&format!("layerfs {}", intent::clearing(&op, record.name()))),
        )),
        Standing::Stale => None,
    }
}

/// `broken` for `reason` on Store `record`, with its fix: a missing file
/// is restored from the first bucket the Store is linked to, or the Store
/// removed when it has no backup; an unreachable server is the doctor's to
/// explain; an unsettled compaction is settled by the next one.
pub fn broken(record: &StoreRecord, reason: Reason) -> State {
    let store = record.name();
    let fix = match reason {
        Reason::Missing => match record.links().first() {
            Some((bucket, link)) => format!(
                "layerfs backup restore {bucket} {} {store}",
                link.file().unwrap_or(store)
            ),
            None => format!("layerfs store remove {store}"),
        },
        Reason::Unreachable => format!("layerfs store doctor {store}"),
        Reason::Unresolved => format!("layerfs store compact {store}"),
        _ => return State::broken(reason, None),
    };
    State::broken(reason, Some(&fix))
}

/// The reason a live read may write to the record ([`SEEN`]) for `word`.
pub fn seen_reason(word: &str) -> Option<Reason> {
    [Reason::Missing, Reason::Unreachable, Reason::Unresolved]
        .into_iter()
        .find(|reason| reason.word() == word)
}

/// What the last live read found wrong with the Store and wrote, if anything.
pub fn seen(record: &StoreRecord) -> Option<State> {
    let reason = seen_reason(record.text(SEEN)?)?;
    Some(broken(record, reason))
}

/// Store `record`'s state from the home's records, `under` being its
/// machine's state for a sqlite Store on a machine (none for a PostgreSQL
/// Store, which is on no machine; a sqlite Store given none is not
/// blocked). A record this build cannot place is `broken`: a kind it
/// does not read is `unsupported`, a sqlite Store whose machine a 1.1.0
/// home did not tell is `missing`.
pub fn recorded_under(record: &StoreRecord, under: Option<&State>) -> Result<State> {
    if let Some(state) = held(record) {
        return Ok(state);
    }
    match record.store_ref() {
        Err(_) => return Ok(State::broken(Reason::Unsupported, None)),
        Ok(StoreRef::Sqlite {
            machine: Placement::Unknown,
            ..
        }) => return Ok(State::broken(Reason::Missing, None)),
        Ok(StoreRef::Sqlite {
            machine: Placement::Machine(machine),
            ..
        }) => {
            if let Some(blocked) = under.and_then(|state| state.blocks("machine", machine)) {
                return Ok(blocked);
            }
        }
        Ok(StoreRef::Postgres { .. }) => {}
    }
    Ok(seen(record).unwrap_or(State::Ready))
}

/// The machine a sqlite Store's record places it on, if it names one.
pub fn placed(record: &StoreRecord) -> Option<&str> {
    match record.store_ref() {
        Ok(StoreRef::Sqlite {
            machine: Placement::Machine(machine),
            ..
        }) => Some(machine),
        _ => None,
    }
}

/// Store `record`'s state from the home's records: the one derivation
/// every package uses for a Store, its machine's state from `machines`.
pub fn recorded(home: &Path, record: &StoreRecord, machines: Machines) -> Result<State> {
    let under = match placed(record) {
        Some(machine) => Some(machines(home, machine)?),
        None => None,
    };
    recorded_under(record, under.as_ref())
}

/// Store `store`'s records-only state by name; a bad name is
/// `InvalidInput`, no record `NotFound`.
pub fn of(home: &Path, store: &str, machines: Machines) -> Result<State> {
    recorded(home, &StoreRecord::of(home, store)?, machines)
}
