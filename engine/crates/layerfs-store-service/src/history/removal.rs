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
use super::lookup::{self as args, refuse, Answer};
use super::MAIN;
use crate::store::Store;
use layerfs_protocol::provision::install::SdkValue;
use layerfs_history::{GcScopeKind, LayerStackRecord};
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
    let entity = branch.name.as_str();
    // A provider without admission still writes the marker: its register is
    // `Unsupported(NO_ADMISSION)`, and there is nothing to release afterwards.
    // Any other `Unsupported` (a pooled connection) refuses (#776 C1).
    let admitted = match catalog.gc_register(&intent) {
        Ok(()) => true,
        Err(error) if error.no_admission() => false,
        // The row's durability is unknown, so no durable op is named.
        Err(error) => {
            return Err(args::history_in(
                error,
                GcScopeKind::Branch,
                entity,
                None,
                None,
            ))
        }
    };
    // A durable op is known only after the registration committed; without
    // one the §10 A6 detail leaves both phase and op null.
    let (phase, durable) = if admitted {
        (Some("retire"), Some(&intent.op_id))
    } else {
        (None, None)
    };
    // Release on every exit path after a successful registration. A failed
    // delete marks the row uncertain instead of leaving it `live`, so a later
    // call reports `Unresolved` rather than a permanent `Busy`.
    let removed = catalog
        .gc_remove_branch(branch.id, removed_ms, &intent)
        .map_err(|error| args::history_in(error, GcScopeKind::Branch, entity, phase, durable));
    let settled = if admitted {
        crate::history::admission::release_or_mark(catalog, &intent.op_id)
            .map_err(|error| args::history_in(error, GcScopeKind::Branch, entity, phase, durable))
    } else {
        Ok(())
    };
    removed?;
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
