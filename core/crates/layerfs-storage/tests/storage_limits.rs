//! Boundary cases for the storage-side record, group and transaction limits.
//!
//! These figures bound what one group body, one group directory and one
//! transaction may hold. Each case here pins the largest accepted value and the
//! smallest refused one, so a limit that is "enforced" in the policy table has a
//! case that fails when its enforcement moves.

mod support;

use layerfs_storage::pack::assemble::frame_group;
use layerfs_storage::pack::layout::{record_range, PackLane};
use layerfs_storage::policy::{
    GROUP_COUNT_LIMIT, GROUP_LIMIT, RECORD_COUNT_LIMIT, SINGLETON_PACK_LIMIT,
};
use layerfs_storage::StorageError;

/// One record of `bytes` bytes.
fn record(bytes: usize) -> Vec<u8> {
    vec![0x41; bytes]
}

#[test]
fn a_group_body_is_accepted_at_its_bound_and_refused_over_it() {
    // A group's framed body is the group ceiling. One record fills it exactly at
    // the bound because the framing is a 4-byte count plus a 4-byte end per row.
    let framing = 4 + 4;
    assert!(GROUP_LIMIT > framing);
    let at_limit = vec![record(GROUP_LIMIT - framing)];
    let framed = frame_group(&at_limit).expect("a body exactly at the group ceiling is accepted");
    assert_eq!(framed.len(), GROUP_LIMIT);
    let over = vec![record(GROUP_LIMIT - framing + 1)];
    assert!(
        matches!(
            frame_group(&over),
            Err(StorageError::CapacityExceeded {
                what: "pack.group_body",
                limit,
                actual,
            }) if limit == GROUP_LIMIT as u64 && actual == (GROUP_LIMIT + 1) as u64
        ),
        "one byte over the group ceiling is refused by that ceiling"
    );
}

#[test]
fn a_group_record_count_is_accepted_at_its_bound_and_refused_over_it() {
    // `RECORD_COUNT_LIMIT` counts rows, not bytes, so the case uses one-byte
    // records and pins the count without depending on the body ceiling.
    assert_eq!(RECORD_COUNT_LIMIT, 8_191);
    // The count has to bind before the body ceiling, or this case would be
    // measuring the byte bound instead of the row bound.
    let count_binds_first = RECORD_COUNT_LIMIT * 2 + 4 <= GROUP_LIMIT;
    assert!(count_binds_first, "the count binds first here");
    let at_limit = vec![record(1); RECORD_COUNT_LIMIT];
    let framed = frame_group(&at_limit).expect("a group at the row ceiling is accepted");
    assert_eq!(
        framed.len(),
        4 + 4 * RECORD_COUNT_LIMIT + RECORD_COUNT_LIMIT
    );
    let over = vec![record(1); RECORD_COUNT_LIMIT + 1];
    assert!(
        matches!(
            frame_group(&over),
            Err(StorageError::Integrity("group record count"))
        ),
        "one row over the count ceiling is refused"
    );
    assert!(
        matches!(
            frame_group(&[]),
            Err(StorageError::Integrity("group record count"))
        ),
        "an empty group is not a group"
    );
}

#[test]
fn a_group_record_directory_is_accepted_at_its_bound_and_refused_over_it() {
    // `record_range` validates the directory it is handed: a count in
    // `1..=RECORD_COUNT_LIMIT`, four end bytes per record that ascend, and a
    // directory inside the pack.
    let count = 2_usize;
    let group_length = 4 + 4 * count + 8;
    let mut ends = Vec::new();
    for end in [4_u32, 8] {
        ends.extend_from_slice(&end.to_le_bytes());
    }
    record_range(count, &ends, group_length, 0).expect("the first record resolves");
    record_range(count, &ends, group_length, 1).expect("the second record resolves");
    assert!(
        matches!(
            record_range(0, &ends, group_length, 0),
            Err(StorageError::Integrity("group record directory"))
        ),
        "a group with no records is refused"
    );
    assert!(
        matches!(
            record_range(count, &ends, group_length, count),
            Err(StorageError::Integrity("group record directory"))
        ),
        "an ordinal outside the directory is refused"
    );
    assert!(
        matches!(
            record_range(count, &ends[..ends.len() - 1], group_length, 0),
            Err(StorageError::Integrity("group record directory"))
        ),
        "a directory whose width does not match its count is refused"
    );
    assert!(
        matches!(
            record_range(count, &ends, SINGLETON_PACK_LIMIT + 1, 0),
            Err(StorageError::Integrity("group record directory"))
        ),
        "a group beyond the pack ceiling is refused"
    );
    let flat = [4_u32.to_le_bytes(), 4_u32.to_le_bytes()].concat();
    assert!(
        matches!(
            record_range(count, &flat, group_length, 0),
            Err(StorageError::Integrity("record end ordering"))
        ),
        "a directory whose ends do not ascend is refused"
    );
}

#[test]
fn the_declared_group_and_record_ceilings_are_the_ones_this_suite_pins() {
    // A limit is only as good as the constant behind it: this case fails when a
    // ceiling moves without its boundary case moving with it, which is how the
    // table and the enforcement stay the same claim.
    assert_eq!(GROUP_LIMIT, 65_536);
    assert_eq!(RECORD_COUNT_LIMIT, 8_191);
    assert_eq!(GROUP_COUNT_LIMIT, 256);
    assert_eq!(
        layerfs_storage::policy::TRANSACTION_ROW_LIMIT,
        RECORD_COUNT_LIMIT as u64,
        "one transaction may carry as many rows as one group may hold"
    );
    assert_eq!(
        layerfs_storage::policy::TRANSACTION_CANONICAL_BYTES_LIMIT,
        4 * 1024 * 1024 - 1
    );
}

#[test]
fn the_group_count_ceiling_is_named_with_its_derivation() {
    // `GROUP_COUNT_LIMIT` is enforced in two places - placement starts a new
    // pack when a lane's open pack would exceed its group-count ceiling
    // (`LanePlacement::group_count_limit` plus `append_fits`), and the pack
    // parser refuses a group count outside `1..=GROUP_COUNT_LIMIT` - but no
    // fixture reaches the ceiling through a real save: filling one pack with
    // 256 groups needs the lane's grouping to fill every group before the
    // 257th is offered, which is a full pack's worth of lane traffic, not a
    // cheap boundary case. The figure is pinned here so the table, the parser
    // and this note stay the same claim; derived, unverified at scale.
    assert_eq!(GROUP_COUNT_LIMIT, 256);
    assert_eq!(PackLane::Ordinary.group_count_limit(), GROUP_COUNT_LIMIT);
    assert_eq!(PackLane::Native.group_count_limit(), GROUP_COUNT_LIMIT);
    assert_eq!(PackLane::WholeFile.group_count_limit(), GROUP_COUNT_LIMIT);
    assert_eq!(
        PackLane::PooledMetadata.group_count_limit(),
        GROUP_COUNT_LIMIT
    );
    assert_eq!(PackLane::Singleton.group_count_limit(), 1);
}
