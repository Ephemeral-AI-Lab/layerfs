//! `branch.remove`: soft-remove one Branch through its Store marker.
//!
//! The marker is written by the catalog in one transaction; this module only
//! resolves the Branch through the filtered visibility surface and renders the
//! result. Branch-scope admission (`MountsActive`/`Busy`) belongs to the call
//! table, which is not wired yet.
use super::lookup::{self as args, history, refuse, Answer};
use super::MAIN;
use crate::store::Store;
use layerfs_bridge::control::SdkValue;
use layerfs_history::LayerStackRecord;
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
    store
        .history()
        .mark_branch_removed(branch.id, removed_ms)
        .map_err(history)?;
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
