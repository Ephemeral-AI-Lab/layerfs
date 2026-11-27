//! The worker side of `project.delete` (its frames in [`super::run`]):
//! `project.delete.admit`, `.release`, the marker itself and
//! `project.delete.entry`, each on a Store under the worker's own root.
use crate::admin::open::args::OpenArgs;
use crate::admin::open::service;
use crate::data::project::frame::{
    DeleteAdmitFrame, DeleteEntryFrame, DeleteEntryReply, DeleteFrame, DeleteReleaseFrame,
    DeleteReply,
};
use crate::data::project::record::project::ProjectRecord;
use layerfs_history::{GcScopeKind, HistoryCatalog, HistoryError, LayerStackId};
use layerfs_home::home::{now_ms, paths};
use layerfs_home::outcome::{failed, refused, Outcome};
use layerfs_protocol::sdk::hex;
use layerfs_protocol::sdk::{SdkRefusal, SdkValue};
use layerfs_store_service::history::admission as store_admission;
use layerfs_store_service::history::deletion::{delete_stack_with, Deletion};
use layerfs_store_service::history::lookup;
use std::path::Path;

use super::run::remove_tree;

type Result<T> = std::result::Result<T, SdkRefusal>;

/// The project name a worker `project.delete*` op records: the caller's, or
/// the Stack record's when the caller sent none, so a later `Busy` or
/// `Unresolved` reports `{kind: "project", name}` rather than the hex
/// LayerStack id. A Stack record that is already gone leaves the id, which is
/// all the caller left to go on.
fn worker_project(
    history: &dyn HistoryCatalog,
    stack: LayerStackId,
    project: Option<String>,
) -> Result<Option<String>> {
    if project.is_some() {
        return Ok(project);
    }
    Ok(history
        .layer_stack(stack)
        .map_err(lookup::history)?
        .map(|record| record.name.as_str().to_owned()))
}

/// The `{kind: "Admitted", stack_id}` answer.
fn admitted(stack: LayerStackId) -> SdkValue {
    SdkValue::map([
        ("kind", SdkValue::text("Admitted")),
        ("stack_id", SdkValue::text(hex(&stack.to_bytes()))),
    ])
}

/// The `{kind: "Released", op_id}` answer.
fn released(op: [u8; 16]) -> SdkValue {
    SdkValue::map([
        ("kind", SdkValue::text("Released")),
        ("op_id", SdkValue::text(hex(&op))),
    ])
}

/// The worker's `project.delete.admit`: registers the caller's project-scope
/// admission before the caller writes its sidecar and before the marker, so a
/// live mount refuses with `MountsActive` (its holders reported) and a live
/// operation with `Busy`, with no sidecar written. The registered row names
/// the project from the frame, or from the Stack record when the caller sent
/// no name, so a later `Busy`/`Unresolved` reports `{kind: "project", name}`
/// rather than the hex LayerStack id.
pub fn admit_project(root: &Path, frame: &DeleteAdmitFrame) -> Outcome {
    let store_record = frame.store_record.clone().ok_or_else(|| {
        refused(
            "InvalidInput",
            "project.delete.admit needs the Store's record",
        )
    })?;
    let open = OpenArgs::in_root(root, frame.store.as_str(), Some(store_record));
    service::with_service(&open, |service| {
        let project = worker_project(service.store.history(), frame.stack, frame.project.clone())?;
        let intent = store_admission::project_removal_at(frame.stack, frame.op, project.as_deref());
        match service.store.history().gc_register(&intent) {
            Ok(()) => Ok(admitted(frame.stack)),
            // A provider without admission cannot exclude a removal; the
            // marker still admits, as it did before T6. A real provider's
            // own `Unsupported` refuses (#776 C1).
            Err(error) if error.no_admission() => Ok(admitted(frame.stack)),
            Err(error) => {
                let entity = project.unwrap_or_else(|| hex(frame.stack.as_slice()));
                Err(lookup::history_in(
                    error,
                    GcScopeKind::Project,
                    &entity,
                    None,
                    Some(&frame.op),
                ))
            }
        }
    })
}

/// The worker's `project.delete.release`: deletes the caller's admission row
/// after the marker, on every exit path.
pub fn release_project(root: &Path, frame: &DeleteReleaseFrame) -> Outcome {
    let store_record = frame.store_record.clone().ok_or_else(|| {
        refused(
            "InvalidInput",
            "project.delete.release needs the Store's record",
        )
    })?;
    let open = OpenArgs::in_root(root, frame.store.as_str(), Some(store_record));
    service::with_service(&open, |service| {
        match store_admission::release_or_mark(service.store.history(), &frame.op) {
            Ok(()) | Err(HistoryError::Unsupported(_)) => Ok(released(frame.op)),
            Err(error) => Err(lookup::history(error)),
        }
    })
}

/// The worker's `project.delete` on a Store under `root`: write the marker, remove the worker's own entry and
/// answer `{kind, project, stack_id, deleted_ms}`. The marker reuses the
/// caller's admission operation identity when its frame carries one, so the
/// caller's admitted row is excluded from its own scan; without one the
/// marker takes a fresh identity and its own admission, as before.
pub fn delete_project(root: &Path, frame: &DeleteFrame) -> Outcome {
    let store = frame.store.as_str();
    let store_record = frame
        .store_record
        .clone()
        .ok_or_else(|| refused("InvalidInput", "project.delete needs the Store's record"))?;
    let open = OpenArgs {
        project: frame.project.clone(),
        ..OpenArgs::in_root(root, store, Some(store_record))
    };
    let op = frame.op_id.unwrap_or_else(store_admission::op_id);
    let requested = frame.deleted_ms.unwrap_or_else(now_ms);
    let deletion = service::with_service(&open, |service| {
        let project = worker_project(service.store.history(), frame.stack, frame.project.clone())?;
        let intent = store_admission::project_removal_at(frame.stack, op, project.as_deref());
        delete_stack_with(&service.store, frame.stack, requested, &intent)
    })?;
    let deleted_ms = deletion.deleted_ms(requested);
    let retired = matches!(deletion, Deletion::Retired { .. });
    // A caller retrying a sidecar written before its admission, over a live
    // row with no marker, settles only: it keeps its home entry, so the
    // worker keeps its own copy for the call that finishes the delete.
    let deferred =
        frame.pending_retry.unwrap_or(false) && matches!(deletion, Deletion::Marked { .. });
    let project = if deferred {
        frame.project.clone().unwrap_or_else(|| store.to_owned())
    } else {
        remove_worker_entry(root, store, frame.stack, frame.project.as_deref())?
            .or_else(|| frame.project.clone())
            .unwrap_or_else(|| store.to_owned())
    };
    Ok(DeleteReply {
        project,
        stack: frame.stack,
        deleted_ms,
        retired: Some(retired),
    }
    .encode())
}

/// The worker's `project.delete.entry`: removes the worker's own home entry
/// for a Stack whose marker is already durable, answering
/// `{kind: "Removed", project, stack_id}`. It opens no Store and writes no
/// marker (spec §5.1).
pub fn remove_project_entry(root: &Path, frame: &DeleteEntryFrame) -> Outcome {
    let store = frame.store.as_str();
    let project = remove_worker_entry(root, store, frame.stack, frame.project.as_deref())?
        .or_else(|| frame.project.clone())
        .unwrap_or_else(|| store.to_owned());
    Ok(DeleteEntryReply {
        project,
        stack: frame.stack,
    }
    .encode())
}

/// Removes the worker's own home entry for `stack`; returns the project name
/// it removed, if any.
#[doc(hidden)]
pub fn remove_worker_entry(
    root: &Path,
    store: &str,
    stack: LayerStackId,
    hint: Option<&str>,
) -> Result<Option<String>> {
    let names = match hint {
        Some(name) => vec![name.to_owned()],
        None => paths::names(&paths::store_dir(root, store).join("project")).map_err(failed)?,
    };
    let mut removed = None;
    for name in names {
        let record = paths::project_record(root, store, &name);
        let Some(value) = layerfs_home::home::read_record(&record)? else {
            continue;
        };
        let matches = ProjectRecord::from_record(store, &name, value).stack_id() == Some(stack)
            || name == store;
        if matches {
            remove_tree(&paths::project_dir(root, store, &name))?;
            removed.get_or_insert(name);
        }
    }
    Ok(removed)
}
