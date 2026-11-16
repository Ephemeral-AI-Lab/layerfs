//! The `project.remove_file` visibility filter's marker-read classification,
//! moved out of `read/visibility.rs`: product source holds no tests (the
//! product-boundary guard).

use layerfs_history::HistoryError;
use layerfs_store_service::read::visibility::unsupported;
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
