//! The daemon's labeled status counts must map onto the fixed wire classes.
//!
//! A Workspace reports its callback counts and both request-size histograms as
//! labeled lists; the wire carries fixed bounded tables. A label the wire does
//! not declare, or a direction the Workspace does not report, is a contract
//! mismatch and must fail closed rather than become a silent zero.
use layerfs_bridge::contract::{
    PROJECTION_BYTE_LABELS, PROJECTION_CLASSES, PROJECTION_CLASS_LABELS, PROJECTION_SIZE_BUCKETS,
    PROJECTION_SIZE_LABELS,
};
use layerfs_daemon::{projection_bytes, projection_counts, projection_sizes};
use layerfs_workspace::WorkspaceStatus;

fn status() -> WorkspaceStatus {
    WorkspaceStatus {
        submission: None,
        generation: 1,
        revision: 1,
        dirty_inodes: 0,
        mounted: true,
        stopping: false,
        closed: false,
        active_operations: 0,
        nodes: 1,
        handles: 0,
        projection_handles: 0,
        projection_replies: 0,
        projection_calls: PROJECTION_CLASS_LABELS
            .iter()
            .enumerate()
            .map(|(index, label)| (*label, index as u64 + 1))
            .collect(),
        projection_bytes: PROJECTION_BYTE_LABELS
            .iter()
            .enumerate()
            .map(|(index, label)| ((*label).to_string(), index as u64 + 1))
            .collect(),
        projection_histogram: ["read", "write"]
            .into_iter()
            .flat_map(|direction| {
                PROJECTION_SIZE_LABELS
                    .iter()
                    .enumerate()
                    .map(move |(index, label)| (format!("{direction}:{label}"), index as u64 + 1))
            })
            .collect(),
        upstream_calls: 7,
        coherence: None,
        cookies: 0,
        accounted_bytes: 0,
    }
}

#[test]
fn every_declared_class_and_bucket_maps_in_order() {
    let local = status();
    let counts = projection_counts(&local).unwrap();
    assert_eq!(counts.len(), PROJECTION_CLASSES);
    assert_eq!(counts[0], 1);
    assert_eq!(counts[PROJECTION_CLASSES - 1], PROJECTION_CLASSES as u64);

    let totals = projection_bytes(&local).unwrap();
    assert_eq!(totals.len(), PROJECTION_BYTE_LABELS.len());
    assert_eq!(totals[3], 4);

    // Both directions arrive in one labeled list and must still land as one
    // bounded table each, in bucket order.
    for direction in ["read", "write"] {
        let sizes = projection_sizes(&local.projection_histogram, direction).unwrap();
        assert_eq!(sizes.len(), PROJECTION_SIZE_BUCKETS);
        assert_eq!(sizes[0], 1);
        assert_eq!(
            sizes[PROJECTION_SIZE_BUCKETS - 1],
            PROJECTION_SIZE_BUCKETS as u64
        );
    }
}

#[test]
fn a_missing_or_unknown_label_fails_closed() {
    let mut short = status();
    short.projection_calls.pop();
    assert!(projection_counts(&short).is_err());

    let mut short = status();
    short.projection_bytes.pop();
    assert!(projection_bytes(&short).is_err());

    let mut unknown = status();
    unknown.projection_bytes[0].0 = "read_requested".to_string();
    assert!(projection_bytes(&unknown).is_err());

    let mut unknown = status();
    unknown.projection_histogram[0].0 = "read:0-2".to_string();
    assert!(projection_sizes(&unknown.projection_histogram, "read").is_err());

    let mut short = status();
    short
        .projection_histogram
        .retain(|(label, _)| !label.starts_with("write:"));
    assert!(projection_sizes(&short.projection_histogram, "write").is_err());
}
