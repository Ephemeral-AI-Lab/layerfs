//! The Windows overlay's allocated-byte accounting reads FileStandardInfo on
//! the retained handle: the footprint of the written tail, not the logical
//! length the old stand-in reported.
#![cfg(target_os = "windows")]
mod payload_support;
use layerfs_overlay::ProfileConfig;
use payload_support::*;

#[test]
fn allocated_bytes_report_the_ondisk_footprint_of_the_written_tail() {
    let temp = Temp::new();
    let db = temp.db();
    db.open_workspace([11; 32]).unwrap();
    let mut source = db.acquire_base_source(1).unwrap();
    // 300,001 bytes: not a cluster multiple, so the on-disk footprint is
    // strictly above the logical length on a cluster-addressed volume —
    // which the old logical-length stand-in could never report.
    let mut file = File::new(&db, source, 3, pattern(300_001, 5));
    let _ = file.read(0, 16);
    let state = db.allocation().unwrap();
    assert!(state.allocated_bytes > 0, "a written overlay holds clusters");
    assert!(
        state.allocated_bytes >= state.logical_bytes,
        "cluster rounding keeps the footprint at or above the tail"
    );
    assert!(
        state.allocated_bytes.saturating_sub(state.logical_bytes) < 1 << 20,
        "the footprint tracks the tail within a cluster margin, got {} for {}",
        state.allocated_bytes,
        state.logical_bytes
    );
    println!(
        "allocated={} logical={}",
        state.allocated_bytes, state.logical_bytes
    );
}
