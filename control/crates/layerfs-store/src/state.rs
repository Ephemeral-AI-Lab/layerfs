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
use layerfs_machine::record::MachineRecord;
use std::path::Path;

type Result<T> = std::result::Result<T, SdkRefusal>;

/// The record field that holds what the last live read found wrong with
/// the Store, as a reason word: `missing`, `unreachable` or `unresolved`.
/// Absent while the Store was found well, or never read live.
pub const SEEN: &str = "broken";

/// A machine's state from its record alone.
///
/// **The seam with the machine family.** This is the single place the
/// Store code derives a machine's records-only state, and it is the
/// machine family's own derivation ([`MachineRecord::status`],
/// [`layerfs_machine::state::unrecorded`]), so a Store's `blocked_by` says
/// what `machine get` says. Two inputs that only `layerfs-control` has are
/// left out: whether the record's platform was removed in 2.0 (the policy
/// is behind the `machine` feature, which this crate's guest build lacks),
/// so a removed platform's machine blocks its Stores by what its record
/// says rather than as `unsupported`; and the Sandbox count, which only
/// shapes an outdated worker's `fix`.
pub fn machine_state(home: &Path, machine: &str) -> Result<State> {
    Ok(match MachineRecord::read(home, machine)? {
        Some(record) => record.status(false, 0),
        None => layerfs_machine::state::unrecorded(machine),
    })
}

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
/// machine's state when the caller already derived it (a list reads each
/// machine once). A record this build cannot place is `broken`: a kind it
/// does not read is `unsupported`, a sqlite Store whose machine a 1.1.0
/// home did not tell is `missing`.
pub fn recorded_under(home: &Path, record: &StoreRecord, under: Option<&State>) -> Result<State> {
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
            let blocked = match under {
                Some(state) => state.blocks("machine", machine),
                None => machine_state(home, machine)?.blocks("machine", machine),
            };
            if let Some(blocked) = blocked {
                return Ok(blocked);
            }
        }
        Ok(StoreRef::Postgres { .. }) => {}
    }
    Ok(seen(record).unwrap_or(State::Ready))
}

/// Store `record`'s state from the home's records: the one derivation
/// every package uses for a Store.
pub fn recorded(home: &Path, record: &StoreRecord) -> Result<State> {
    recorded_under(home, record, None)
}

/// Store `store`'s records-only state by name; a bad name is
/// `InvalidInput`, no record `NotFound`.
pub fn of(home: &Path, store: &str) -> Result<State> {
    recorded(home, &StoreRecord::of(home, store)?)
}
