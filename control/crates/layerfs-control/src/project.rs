//! The `project` calls that need this layer's reads of other entities:
//! `project.delete`, which retires a project in its Store and removes the
//! caller's home entry (spec §4), with the project-scope `Busy` checks of
//! the Sandboxes that use it; and the project reads, which take this
//! layer's machine state (`project.get`, `project.list`). The other
//! `project` calls are `layerfs-store`'s own.
use crate::inventory;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_home::events::Events;
use layerfs_home::names::ProjectRef;
use layerfs_home::op::Standing;
use layerfs_home::outcome::Outcome;
use layerfs_store::project::op::{self, DELETE, INIT};
use layerfs_store::project::read::{self, ProjectList};
use layerfs_store::project::record::ProjectRecord;
use std::path::Path;

/// The project-scope `Busy` checks `project.delete` takes from this layer.
pub const PROJECT_RELATED: layerfs_store::Related = layerfs_store::Related {
    store_idle: inventory::store_idle,
    can_remove_project: inventory::can_remove_project,
};

/// `project.get(project)`: one project, its state and main's head; its
/// Store's machine state is the inventory's.
pub fn get(home: &Path, project: &ProjectRef) -> Outcome {
    read::get(home, project, &crate::store::machine_state)
}

/// `project.list({ store, limit, cursor, with: stats })`.
pub fn list(home: &Path, request: &ProjectList) -> Outcome {
    read::list(home, request, &crate::store::machine_state)
}

/// `project.delete(project)`: retires the project in its Store and removes
/// the caller's home entry (spec §4), with the delete's `op` in the
/// project's record while it runs: a certain failure drops it, an unknown
/// outcome keeps it uncertain (`broken` `unresolved` until the delete is
/// run again). An Init that stopped part-way and never reached the Store
/// is cleared by removing its entry, and so is any entry whose project the
/// Store's own list does not hold (deleted from another home; the fix
/// `project doctor` names). An Init through a machine's worker whose
/// outcome is not known, recorded with no stack id, is resolved by that
/// Store's list, read once: retired there when it is listed. Answers `{ ok, home_entry_kept,
/// outcome, deleted_at }`; a home entry it could not remove is kept for the
/// next run, the project being retired already.
pub fn delete(home: &Path, events: &mut Events<'_>, project: &ProjectRef) -> Outcome {
    let (store, name) = (project.store.as_str(), project.project.as_str());
    let prior = ProjectRecord::read(home, store, name)?.and_then(|record| record.op());
    let left_init = prior.is_some_and(|op| op.name() == INIT && op.standing() != Standing::Running);
    op::begin(home, store, name, DELETE, "mark")?;
    let outcome =
        match layerfs_store::project::delete::delete(home, events, project, PROJECT_RELATED) {
            Err(refusal) if refusal.code == "NoSuchProject" => {
                // a list that cannot be read leaves the Store's answer and
                // the entry as they are
                let clear = left_init || unlisted(home, store, name).unwrap_or(false);
                match clear {
                    true => op::remove_entry(home, store, name)
                        .map(|()| removed(layerfs_home::home::now_ms(), true)),
                    false => Err(refusal),
                }
            }
            Ok(value) => {
                let deleted_ms = value
                    .get_int("deleted_ms")
                    .unwrap_or_else(layerfs_home::home::now_ms);
                Ok(removed(deleted_ms, value.get_bool("home_entry_removed")))
            }
            Err(refusal) => Err(refusal),
        };
    settle(home, store, name, &outcome);
    outcome
}

/// Whether the home records project `name` of Store `store` and the Store's
/// own list does not hold it, by the record's stack id or, for a record
/// without one, by its name (the `project.listed` comparison `project
/// doctor` makes, whose fix is `project delete`): an entry left by a delete
/// from another home, or by an Init that never reached the Store. A list
/// that cannot be read is its refusal.
fn unlisted(home: &Path, store: &str, name: &str) -> Result<bool, SdkRefusal> {
    let (Some(record), Some(project)) = (
        layerfs_store::record::StoreRecord::read(home, store)?,
        ProjectRecord::read(home, store, name)?,
    ) else {
        return Ok(false);
    };
    let projects = layerfs_store::project::live::projects(home, store, &record, false)?;
    Ok(layerfs_store::project::live::find(&project, name, &projects).is_none())
}

/// The delete's answer.
fn removed(deleted_ms: i64, home_entry_removed: bool) -> SdkValue {
    SdkValue::map([
        ("ok", SdkValue::Bool(true)),
        ("home_entry_kept", SdkValue::Bool(!home_entry_removed)),
        ("outcome", SdkValue::text("deleted")),
        (
            "deleted_at",
            SdkValue::Text(layerfs_home::time::iso(deleted_ms)),
        ),
    ])
}

/// Settles the delete's `op` by its outcome: `Unknown` keeps it uncertain,
/// anything else drops it from a record that is still there.
fn settle(home: &Path, store: &str, name: &str, outcome: &Result<SdkValue, SdkRefusal>) {
    let _ = match outcome {
        Err(refusal) if refusal.code == "Unknown" => {
            op::uncertain(home, store, name, &refusal.message, true)
        }
        _ => op::end(home, store, name),
    };
}
