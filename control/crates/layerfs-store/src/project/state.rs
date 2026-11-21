//! A project's state (call surface 2.0): `ready`, `busy` (an Init or a
//! delete runs: `op`), `blocked` (its Store, or that Store's machine, is
//! not ready: `blocked_by`) or `broken` `unresolved` (a delete, or an Init,
//! that stopped part-way, fixed by `layerfs project delete <project>`). A
//! project is never `stopped` or `unknown`.
//!
//! In order: the operation the project's record holds ([`held`]); the
//! Store's records-only state, which blocks the project when it is not
//! ready; the Store's own list, which hides a project it does not list and
//! blocks every project of a Store it cannot be read from; a record a local
//! sqlite project cannot use ([`ProjectRecord::ready`]: no `manifest` or
//! `installed`, or one that does not decode), which is `broken` `failed`
//! with the project's delete as its fix; a delete intent left behind
//! (`store/<store>/delete/<project>.json`) on a project the Store still
//! lists, which is `broken` `unresolved`; else `ready`.
use crate::project::delete::Intent;
use crate::project::frame::ListedProject;
use crate::project::op::{DELETE, INIT};
use crate::project::record::ProjectRecord;
use crate::record::StoreRecord;
use crate::state::Machines;
use layerfs_bridge::control::SdkRefusal;
use layerfs_home::op::Standing;
use layerfs_home::outcome::refused;
use layerfs_home::outcome::shell_word;
use layerfs_home::outcome::FIX;
use layerfs_home::state::{Reason, State};
use std::path::Path;

type Result<T> = std::result::Result<T, SdkRefusal>;

/// `broken` `unresolved` for project `store`/`name`, with its fix.
pub fn unresolved(store: &str, name: &str) -> State {
    let fix = format!(
        "layerfs project delete {store}/{name}",
        store = shell_word(store),
        name = shell_word(name)
    );
    State::broken(Reason::Unresolved, Some(&fix))
}

/// The state the project's record's own operation gives it, if it holds
/// one that still means something: `busy` while its owner lives; `broken`
/// `unresolved` for an Init whose owner is gone (an Init ends by writing
/// its record without the `op`, or by removing the entry, so one left
/// behind did not end) and for a delete its gone owner left uncertain. A
/// gone owner's certain delete holds nothing.
pub fn held(store: &str, name: &str, record: &ProjectRecord) -> Option<State> {
    let op = record.op()?;
    match (op.standing(), op.name()) {
        (Standing::Running, INIT | DELETE) => Some(State::Busy {
            name: op.word().to_owned(),
            phase: op.phase().to_owned(),
            started_ms: op.started_ms(),
        }),
        (Standing::Running, _) | (Standing::Stale, DELETE) => None,
        (_, INIT) | (Standing::Unresolved, DELETE) => Some(unresolved(store, name)),
        _ => None,
    }
}

/// Whether a delete of project `store`/`name` left its intent behind.
pub fn intent_left(home: &Path, store: &str, name: &str) -> bool {
    Intent::path(home, store, name).exists()
}

/// What the Store's own list says of a project.
#[derive(Clone, Copy, Debug)]
pub enum Listing<'a> {
    /// The list was not read: the Store is blocked, or was not asked.
    Skipped,
    /// The Store could not be read; the reason it blocks its projects with.
    Unread(&'static str),
    /// The Store does not list the project.
    Hidden,
    /// The Store lists it.
    Listed(&'a ListedProject),
}

/// The state of project `name` of `store` (see the module), its Store's
/// block `block` (the Store's records-only state, when it is not ready) and
/// what the Store's list says. `None` is a project the Store does not list:
/// it is hidden, as today.
pub fn derive(
    home: &Path,
    store: &StoreRecord,
    name: &str,
    record: &ProjectRecord,
    block: Option<&State>,
    listing: Listing<'_>,
) -> Option<State> {
    let store_name = store.name();
    if let Some(state) = held(store_name, name, record) {
        return Some(state);
    }
    if let Some(block) = block {
        return Some(block.clone());
    }
    match listing {
        Listing::Unread(reason) => Some(State::blocked_by("store", store_name, reason)),
        Listing::Hidden => None,
        Listing::Skipped | Listing::Listed(_) => {
            // a record this build cannot use is `broken` `failed` (c7),
            // after the Store's block and before the delete intent: the
            // record exists, so its entry (not a bare folder) is the fix
            if let Err(refusal) = record.ready(home, store) {
                let fix = refusal.detail.get_text(FIX);
                return Some(State::broken(Reason::Failed, fix));
            }
            Some(match intent_left(home, store_name, name) {
                true => unresolved(store_name, name),
                false => State::Ready,
            })
        }
    }
}

/// The block a Store `record` puts on its projects, from the home's records
/// alone: its state when that is not ready (its machine's from `machines`).
pub fn store_block(home: &Path, record: &StoreRecord, machines: Machines) -> Result<Option<State>> {
    let state = crate::state::recorded(home, record, machines)?;
    Ok(state.blocks("store", record.name()))
}

/// The reason a Store whose list could not be read blocks its projects
/// with, from the refusal: `busy`, `missing` (no Store file), `unsupported`
/// (this host cannot open it), `unresolved`, `not_answering` (a worker's
/// lost reply) or `unreachable`.
pub fn unread(refusal: &SdkRefusal) -> &'static str {
    match refusal.code.as_str() {
        "Busy" => "busy",
        "NoStore" | "NotFound" | "NoSuchStore" => Reason::Missing.word(),
        "Unsupported" => Reason::Unsupported.word(),
        "Unresolved" => Reason::Unresolved.word(),
        "Unknown" => Reason::NotAnswering.word(),
        _ => Reason::Unreachable.word(),
    }
}

/// Project `store`/`name`'s state from the home's records alone, for a
/// caller that does not read the Store's list (the home tree, a parent's
/// doctor): its record's operation, its Store's block, a delete intent left
/// behind, else `ready`. Without the Store's list it cannot tell a project
/// the Store no longer lists. No record is `NoSuchProject`.
pub fn recorded(home: &Path, store: &StoreRecord, name: &str, machines: Machines) -> Result<State> {
    let store_name = store.name();
    let record = ProjectRecord::read(home, store_name, name)?
        .ok_or_else(|| refused("NoSuchProject", format!("no project {store_name}/{name}")))?;
    let block = store_block(home, store, machines)?;
    let state = derive(home, store, name, &record, block.as_ref(), Listing::Skipped);
    Ok(state.unwrap_or(State::Ready))
}
