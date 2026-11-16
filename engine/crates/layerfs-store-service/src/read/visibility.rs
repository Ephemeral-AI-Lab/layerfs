//! The `project.remove_file` visibility filter over one resolved snapshot.
//!
//! A path is hidden at node `N` when its entry exists at `N` and the marker
//! frontier, the maximum over the path and its `/`-boundary ancestors, is at
//! least `N`'s `history_serial` (`§4.3`). A marked-but-absent path hides
//! nothing, so callers check existence first. Reads and stats refuse with one
//! `Removed` detail; listings and globs omit hidden entries and report one
//! top-level `removed_count`.

use crate::read::snapshot::Snap;
use crate::store::Store;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_history::error::{RemovedPath, RemovedState};
use layerfs_history::{HistoryError, LayerStackRecord, PathRemovalRecord};
use std::sync::Arc;

/// The markers and node position one snapshot's reads filter against.
pub struct Visibility {
    serial: u64,
    markers: Vec<PathRemovalRecord>,
}

impl Visibility {
    /// Loads the snapshot's serial and its project's markers. A provider that
    /// does not implement the marker reads (a test double) sees none.
    pub fn load(store: &Arc<Store>, stack: &LayerStackRecord, snap: &Snap) -> Self {
        let serial = snap.serial;
        let markers = match store.published(|history| history.path_removals(stack.id)) {
            Ok(markers) => markers,
            Err(_) => Vec::new(),
        };
        Self { serial, markers }
    }

    /// The governing frontier for a path: the maximum marker frontier over the
    /// path and its `/`-boundary ancestors, if any marker applies.
    fn frontier(&self, path: &str) -> Option<u64> {
        let relative = path.strip_prefix('/').unwrap_or(path);
        if relative.is_empty() {
            return None;
        }
        self.markers
            .iter()
            .filter(|marker| layerfs_history::path::governs(&marker.path, relative.as_bytes()))
            .map(|marker| marker.frontier_serial)
            .max()
    }

    /// The governing frontier when the entry at `path` is hidden at this node.
    pub fn hidden(&self, path: &str) -> Option<u64> {
        let frontier = self.frontier(path)?;
        (frontier >= self.serial).then_some(frontier)
    }

    /// The refusal for one hidden read or stat, `{paths: [{path, frontier_serial}]}`.
    pub fn refusal(path: &str, frontier_serial: u64) -> SdkRefusal {
        crate::history::lookup::history(HistoryError::Removed(Box::new(RemovedState::new(vec![
            RemovedPath::new(path, frontier_serial),
        ]))))
    }
}

/// The `Removed` refusal a hidden read or stat reports.
pub fn refuse(path: &str, visibility: &Visibility) -> Option<SdkRefusal> {
    visibility
        .hidden(path)
        .map(|frontier| Visibility::refusal(path, frontier))
}

/// Removes hidden entries from a list or glob answer and adds `removed_count`.
/// Entries themselves exist, so only marked-and-present paths are counted.
pub fn filter(mut value: SdkValue, visibility: &Visibility) -> SdkValue {
    let mut removed = 0i64;
    if let SdkValue::Map(fields) = &mut value {
        if let Some((_, SdkValue::List(items))) = fields.iter_mut().find(|(k, _)| k == "items") {
            items.retain(|item| {
                let path = match item {
                    SdkValue::Text(path) => path.clone(),
                    other => other
                        .get_text("path")
                        .map(str::to_owned)
                        .unwrap_or_default(),
                };
                if path.is_empty() {
                    return true;
                }
                if visibility.hidden(&path).is_some() {
                    removed += 1;
                    false
                } else {
                    true
                }
            });
        }
        fields.push(("removed_count".into(), SdkValue::Int(removed)));
    }
    value
}
