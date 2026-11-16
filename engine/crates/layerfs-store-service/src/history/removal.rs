//! `branch.remove`: soft-remove one Branch through its Store marker.
//!
//! The marker is written by the catalog in one transaction that also admits
//! the Branch scope: a live mount is `MountsActive`, a live operation is
//! `Busy`, and an uncertain owner is `Unresolved`.
//!
//! The call's own retire-phase intent is registered *before* that marker, in
//! its own write transaction: a row written inside the marker transaction
//! commits atomically with the marker and could never be seen by a concurrent
//! remove. It is released after the marker, so a concurrent remove meets it as
//! `Busy` with `phase: "retire"` (spec §4.4, §5.1).
use super::lookup::{self as args, history, refuse, Answer};
use super::MAIN;
use crate::store::Store;
use layerfs_bridge::control::SdkValue;
use layerfs_history::{HistoryError, LayerStackRecord};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

/// `branch.remove(project, branch)`: `{kind: "Removed", project, branch, removed_ms}`.
pub fn remove(store: &Arc<Store>, stack: &LayerStackRecord, name: &str) -> Answer {
    if name == MAIN {
        return Err(refuse(
            "ReservedName",
            "\"main\" names the project's main line",
            SdkValue::Map(Vec::new()),
        ));
    }
    let branch = args::branch(store, stack, name)?;
    let removed_ms = now_ms();
    let intent = crate::history::admission::branch_removal(stack, &branch);
    let catalog = store.history();
    // A provider without admission still writes the marker: its register is
    // `Unsupported`, and there is nothing to release afterwards.
    let admitted = match catalog.gc_register(&intent) {
        Ok(()) => true,
        Err(HistoryError::Unsupported(_)) => false,
        Err(error) => return Err(history(error)),
    };
    let removed = catalog.gc_remove_branch(branch.id, removed_ms, &intent);
    // Release on every exit path after a successful registration. A failed
    // delete marks the row uncertain instead of leaving it `live`, so a later
    // call reports `Unresolved` rather than a permanent `Busy`.
    let settled = if admitted {
        crate::history::admission::release_or_mark(catalog, &intent.op_id).map_err(history)
    } else {
        Ok(())
    };
    removed.map_err(history)?;
    settled?;
    Ok(SdkValue::map([
        ("kind", SdkValue::text("Removed")),
        ("project", SdkValue::text(stack.name.as_str())),
        ("branch", SdkValue::text(branch.name.as_str())),
        ("removed_ms", SdkValue::Int(removed_ms)),
    ]))
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_millis() as i64)
}
