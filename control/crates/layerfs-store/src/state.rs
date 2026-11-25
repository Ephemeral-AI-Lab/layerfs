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
use crate::record::{StoreRecord, StoreRef};
use layerfs_bridge::control::SdkRefusal;
use layerfs_home::op::Standing;
use layerfs_home::outcome::shell_word;
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
/// has no in-place restore (`backup restore` protects the recorded Store
/// from replacement it cannot prove unchanged), or the Store removed when
/// it has no backup; an unsettled compaction is settled by the next one.
/// An unreachable server has no fix: no command makes it answer, and
/// re-running the doctor only observes it again.
pub fn broken(record: &StoreRecord, reason: Reason) -> State {
    let store = record.name();
    let fix = match reason {
        Reason::Missing if record.links().is_empty() => {
            format!("layerfs store remove {store}", store = shell_word(store))
        }
        Reason::Unresolved => format!("layerfs store compact {store}", store = shell_word(store)),
        _ => return State::broken(reason, None),
    };
    State::broken(reason, Some(&fix))
}

/// The code a Store call is refused with when its Store is of an older
/// schema version this release neither reads nor migrates (#776): folded
/// into `Unsupported` at the call boundary, its message the version the
/// Store was refused by, its fix [`outdated_fix`]. A project of such a
/// Store is `blocked` by it, `outdated` (`project::state::unread`).
pub const OLD_STORE: &str = "OldStore";

/// The fix of older-format Store `store`: this build cannot open it, so it
/// goes, and a new Store takes its place. A sqlite Store is removed with
/// its file; a PostgreSQL Store is disconnected, which keeps its data for
/// the earlier build that wrote it.
pub fn outdated_fix(store: &str, postgresql: bool) -> String {
    let call = match postgresql {
        true => "disconnect",
        false => "remove",
    };
    format!("layerfs store {call} {}", shell_word(store))
}

/// The older-format Store status `refusal` carries: its message when it is
/// [`OLD_STORE`], else the version refusal its message wraps (a daemon's
/// `Store open: ...` text), if any.
pub fn outdated_status(refusal: &SdkRefusal) -> Option<String> {
    match refusal.code == OLD_STORE {
        true => Some(refusal.message.clone()),
        false => layerfs_persistence::outdated_store(&refusal.message).map(str::to_owned),
    }
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
/// blocked). A record this build cannot place (a kind it does not read)
/// is `broken` `unsupported`.
pub fn recorded_under(record: &StoreRecord, under: Option<&State>) -> Result<State> {
    if let Some(state) = held(record) {
        return Ok(state);
    }
    match record.store_ref() {
        Err(_) => {
            let fix = crate::record::unread_fix(record.name());
            return Ok(State::broken(Reason::Unsupported, Some(&fix)));
        }
        Ok(StoreRef::Sqlite { machine, .. }) => {
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
        Ok(StoreRef::Sqlite { machine, .. }) => Some(machine),
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
