//! The `project.hide_path` visibility filter's marker-read classification,
//! moved out of `read/visibility.rs`: product source holds no tests (the
//! product-boundary guard).

use layerfs_history::HistoryError;
use layerfs_store_service::filesystem::tree::visibility::unsupported;
use layerfs_store_service::store::PortError;
use std::sync::Arc;

/// A provider without the marker bodies sees none; every other read failure
/// is a refusal, never an empty marker set (fail closed).
#[test]
fn only_unsupported_marker_reads_mean_no_markers() {
    assert!(unsupported(&Arc::new(PortError::History(
        HistoryError::Unsupported("path_removals")
    ))));
    assert!(!unsupported(&Arc::new(PortError::History(
        HistoryError::Busy
    ))));
    assert!(!unsupported(&Arc::new(PortError::Poisoned)));
}

/// The #818 seam: markers already in memory filter exactly as read ones do.
/// The governing frontier is the maximum over the path and its `/`-boundary
/// ancestors, and it hides an entry at a node whose serial is at or below it.
#[test]
fn markers_in_memory_filter_as_read_ones() {
    use layerfs_history::PathRemovalRecord;
    use layerfs_store_service::filesystem::tree::visibility::Visibility;
    let marker = |path: &str, frontier_serial| PathRemovalRecord {
        path: path.as_bytes().to_vec(),
        frontier_serial,
        removed_ms: 0,
    };
    let seen = Visibility::from_markers(7, vec![marker("a", 7), marker("a/b", 9)]);
    assert_eq!(seen.serial(), 7);
    assert_eq!(seen.hidden("/a"), Some(7));
    assert_eq!(seen.hidden("/a/b/c"), Some(9));
    assert_eq!(seen.hidden("/ab"), None, "a sibling with a shared prefix");
    assert_eq!(seen.hidden("/"), None);
    assert_eq!(seen.hidden_at("/a", 8), None, "a later node sees the path");
    assert_eq!(seen.hidden_at("/a/b", 8), Some(9));
    let none = Visibility::from_markers(0, Vec::new());
    assert_eq!((none.serial(), none.hidden("/a")), (0, None));
}
