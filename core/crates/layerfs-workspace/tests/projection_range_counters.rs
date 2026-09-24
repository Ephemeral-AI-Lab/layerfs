use layerfs_workspace::filesystem::projection_counters::{ProjectionCounters, ProjectionOp};

#[test]
fn range_counts_keep_callbacks_and_physical_bytes_distinct() {
    let mut counts = ProjectionCounters::default();
    counts.record(ProjectionOp::RangeState);
    counts.record(ProjectionOp::RangeEdit);
    counts.record(ProjectionOp::RangeEdit);
    counts.record_range_publication(u64::MAX - 1, 0);
    counts.record_range_publication(2, 7);

    assert_eq!(ProjectionOp::RangeState.label(), "range_state");
    assert_eq!(ProjectionOp::RangeEdit.label(), "range_edit");
    assert_eq!(counts.count(ProjectionOp::RangeState), 1);
    assert_eq!(counts.count(ProjectionOp::RangeEdit), 2);
    assert_eq!(counts.range_accepted_payload_bytes(), u64::MAX);
    assert_eq!(counts.range_shifted_suffix_bytes(), 7);
}
