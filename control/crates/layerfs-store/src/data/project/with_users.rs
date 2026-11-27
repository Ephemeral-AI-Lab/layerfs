//! The `project` calls that need this layer's reads of other entities:
//! `project.delete`, which retires a project in its Store and removes the
//! caller's home entry (spec §4), with the project-scope `Busy` checks of
//! the Sandboxes that use it; and the project reads, which take this
//! layer's machine state (`project.get`, `project.list`). The other
//! `project` calls are `layerfs-store`'s own.
use crate::data::project::record::intent::{self, DELETE, INIT};
use crate::data::project::record::project::ProjectRecord;
use crate::data::project::status::get;
use crate::data::project::status::list::{self, ProjectList};
use crate::StoreUsers;
use layerfs_home::events::Events;
use layerfs_home::names::ProjectRef;
use layerfs_home::op::Standing;
use layerfs_home::outcome::Outcome;
use layerfs_protocol::sdk::{SdkRefusal, SdkValue};
use std::path::Path;

/// `project.get(project)`: one project, its state and main's head; its
/// Store's machine state is the inventory's.
pub fn get(home: &Path, project: &ProjectRef, users: &dyn StoreUsers) -> Outcome {
    get::get(home, project, &|home: &Path, machine: &str| {
        users.machine_state(home, machine)
    })
}

/// `project.list({ store, limit, cursor, with: stats })`.
pub fn list(home: &Path, request: &ProjectList, users: &dyn StoreUsers) -> Outcome {
    list::list(home, request, &|home: &Path, machine: &str| {
        users.machine_state(home, machine)
    })
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
/// Store's list, read once: retired there when it is listed. Settling such
/// an Init also removes the folder copy it sent ([`discard_copy`], spec
/// v20). Answers `{ ok, home_entry_kept,
/// outcome, deleted_at }`; a home entry it could not remove is kept for the
/// next run, the project being retired already.
pub fn delete(
    home: &Path,
    events: &mut Events<'_>,
    project: &ProjectRef,
    users: &dyn StoreUsers,
) -> Outcome {
    let (store, name) = (project.store.as_str(), project.project.as_str());
    let prior = ProjectRecord::read(home, store, name)?.and_then(|record| record.op());
    let left_init = prior
        .as_ref()
        .is_some_and(|op| op.name() == INIT && op.standing() != Standing::Running);
    intent::begin(home, store, name, DELETE, "mark")?;
    let outcome =
        match crate::data::project::delete::run::delete(home, events, project, users.related()) {
            Err(refusal) if refusal.code == "NoSuchProject" => {
                // a list that cannot be read leaves the Store's answer and
                // the entry as they are
                let clear = left_init || unlisted(home, store, name).unwrap_or(false);
                match clear {
                    true => intent::remove_entry(home, store, name)
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
    // an Init this process ran ended once it was marked uncertain
    let settled = prior.as_ref().filter(|op| {
        op.name() == INIT && (op.standing() != Standing::Running || (op.mine() && op.uncertain()))
    });
    if let (Ok(_), Some(init)) = (&outcome, settled) {
        discard_copy(home, (store, name), init.args());
    }
    outcome
}

/// Removes the folder copy an Init whose owner is gone sent to a machine
/// (`args`: its `op`'s `{ copy, machine }`), once the delete that settles
/// that Init succeeded (spec v20): one `project.stage` `discard` there. A
/// copy its Init still reads is held by that Init, which removes it when it
/// ends, and is left. What was removed, and a removal that failed, is a
/// line of the home's `log/sweep.jsonl`; the delete's outcome does not
/// change.
fn discard_copy(home: &Path, (store, name): (&str, &str), args: &SdkValue) {
    use crate::admin::route::executor::{on_machine, Executor};
    use crate::data::project::frame::{StageFrame, StageStep};
    let (Some(copy), Some(machine)) = (args.get_text("copy"), args.get_text("machine")) else {
        return;
    };
    let frame = StageFrame {
        store: store.to_owned(),
        name: name.to_owned(),
        id: copy.to_owned(),
        step: StageStep::Discard,
    };
    let discarded = match on_machine(home, machine) {
        Ok(Executor::Worker(mut worker)) => {
            (worker.attempt("project.stage", frame.encode(), &mut |_| {}))
                .map_err(|unreached| unreached.0)
                .and_then(|outcome| outcome)
        }
        Ok(Executor::Local) => crate::data::project::init::staged::run(home, &frame),
        Err(refusal) => Err(refusal),
    };
    let shown = crate::data::project::init::send::left(machine, copy);
    let log =
        |outcome| layerfs_home::home::log_swept(home, "init_copy", Path::new(&shown), &outcome);
    match discarded {
        Ok(out) if out.get_bool("discarded") => log(Ok(None)),
        // absent already, or held by the Init that still reads it
        Ok(_) => {}
        Err(refusal) => log(Err(refusal.message)),
    }
}

/// Whether the home records project `name` of Store `store` and the Store's
/// own list does not hold it, by the record's stack id or, for a record
/// without one, by its name (the `project.listed` comparison `project
/// doctor` makes, whose fix is `project delete`): an entry left by a delete
/// from another home, or by an Init that never reached the Store. A list
/// that cannot be read is its refusal.
fn unlisted(home: &Path, store: &str, name: &str) -> Result<bool, SdkRefusal> {
    let (Some(record), Some(project)) = (
        crate::admin::record::store::StoreRecord::read(home, store)?,
        ProjectRecord::read(home, store, name)?,
    ) else {
        return Ok(false);
    };
    let projects = crate::data::project::status::live::projects(home, store, &record, false)?;
    Ok(crate::data::project::status::live::find(&project, name, &projects).is_none())
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
            intent::uncertain(home, store, name, &refusal.message, true)
        }
        _ => intent::end(home, store, name),
    };
}
