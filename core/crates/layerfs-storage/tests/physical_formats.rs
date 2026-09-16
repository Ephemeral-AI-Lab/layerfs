//! Explicit physical format matrix: recognized framings and rejected ones.
//!
//! A reader never trial-decodes. Each framing this slice writes is recognized by
//! its version, each framing it does not implement is refused with a named
//! unsupported-format failure, and the declared allocation, frame and pack bounds
//! are enforced on the bytes rather than assumed.

mod support;

use layerfs_storage::pack::layout::{
    parse_header, PackLane, PACK_MAGIC, VERSION_NATIVE, VERSION_ORDINARY, VERSION_POOLED_METADATA,
    VERSION_SINGLETON, VERSION_WHOLE_FILE,
};
use layerfs_storage::StorageError;

/// Builds a minimal pack header for `version` with `groups` directory entries.
fn header(version: u32, groups: u32, length: usize) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&PACK_MAGIC);
    bytes.extend_from_slice(&version.to_le_bytes());
    bytes.extend_from_slice(&groups.to_le_bytes());
    bytes.resize(length, 0);
    bytes
}

#[test]
fn every_implemented_framing_is_recognized_by_its_own_version() {
    for (version, lane) in [
        (VERSION_ORDINARY, PackLane::Ordinary),
        (VERSION_NATIVE, PackLane::Native),
        (VERSION_WHOLE_FILE, PackLane::WholeFile),
        (VERSION_POOLED_METADATA, PackLane::PooledMetadata),
        (VERSION_SINGLETON, PackLane::Singleton),
    ] {
        let entry = if lane == PackLane::WholeFile { 4 } else { 16 };
        let bytes = header(version, 1, 16 + entry + 32);
        let parsed = parse_header(&bytes).unwrap_or_else(|error| panic!("{lane:?}: {error}"));
        assert_eq!(parsed.lane, lane);
        assert_eq!(parsed.group_count, 1);
    }
}

#[test]
fn unimplemented_and_unknown_framings_are_rejected_without_a_trial_decode() {
    for version in [0_u32, 3, 5, 8, 99] {
        let bytes = header(version, 1, 16 + 16 + 32);
        let error = parse_header(&bytes).unwrap_err();
        assert!(
            matches!(error, StorageError::UnsupportedPolicy { field } if field == "pack framing version"),
            "version {version}: {error}"
        );
    }
    // A recognized version with a malformed control area is an integrity failure,
    // not a fallback to another framing.
    let mut wrong_magic = header(VERSION_ORDINARY, 1, 80);
    wrong_magic[0] = b'X';
    assert!(matches!(
        parse_header(&wrong_magic),
        Err(StorageError::Integrity(_))
    ));
    let truncated = header(VERSION_ORDINARY, 4, 32);
    assert!(matches!(
        parse_header(&truncated),
        Err(StorageError::Integrity(_))
    ));
}

#[test]
fn declared_pack_and_group_bounds_are_enforced_on_the_bytes() {
    // The ordinary lane is bounded by the 256-KiB pack limit.
    let oversized = header(VERSION_ORDINARY, 1, 256 * 1024 + 1);
    assert!(matches!(
        parse_header(&oversized),
        Err(StorageError::Integrity("pack length"))
    ));
    // The singleton lane is bounded by its own, larger limit.
    let singleton = header(VERSION_SINGLETON, 1, 16 * 1024 * 1024 + 4_096 + 1);
    assert!(matches!(
        parse_header(&singleton),
        Err(StorageError::Integrity("pack length"))
    ));
    // A singleton pack holds exactly one group.
    let two_groups = header(VERSION_SINGLETON, 2, 16 + 32 + 64);
    assert!(matches!(
        parse_header(&two_groups),
        Err(StorageError::Integrity("singleton pack group count"))
    ));
    // A zero group count is not a valid directory.
    let empty = header(VERSION_NATIVE, 0, 64);
    assert!(matches!(
        parse_header(&empty),
        Err(StorageError::Integrity("pack group count"))
    ));
}

#[test]
fn the_lane_table_names_one_limit_per_lane() {
    assert_eq!(PackLane::Singleton.pack_limit(), 16 * 1024 * 1024 + 4_096);
    assert_eq!(PackLane::Ordinary.pack_limit(), 256 * 1024);
    assert_eq!(PackLane::WholeFile.records_per_group(), 1);
    assert_eq!(PackLane::PooledMetadata.records_per_group(), 1);
    assert_eq!(PackLane::Singleton.group_count_limit(), 1);
    assert_eq!(PackLane::PooledMetadata.body_limit(), 16 * 1024);
    for lane in PackLane::ALL {
        if lane != PackLane::Singleton {
            assert_eq!(lane.pack_limit(), 256 * 1024, "{lane:?}");
        }
        assert!(lane.body_limit() <= lane.pack_limit(), "{lane:?}");
    }
}

#[test]
fn a_recipe_frame_larger_than_its_profile_is_refused() {
    // The codec profile is derived from the policy; an oversized raw payload is
    // refused with a named capacity failure rather than a truncated frame.
    use layerfs_storage::encoding::{CodecProfile, CompressionWorkspace};
    let small = layerfs_storage::StoragePolicy::frozen_default();
    let capacities = layerfs_storage::StorageCapacities::from_policy(small).expect("capacities");
    let profile = CodecProfile::whole_file(&capacities);
    assert_eq!(profile.raw_limit(), 131_071);
    assert_eq!(profile.frame_limit(), 135_168);
    let mut workspace = CompressionWorkspace::new().expect("workspace");
    let raw = support::noise(profile.raw_limit() + 1);
    let error = workspace.compress(profile, &raw).unwrap_err();
    assert!(
        matches!(error, StorageError::CapacityExceeded { what, .. } if what == "codec.raw_payload"),
        "got {error}"
    );
    let large = layerfs_storage::StorageCapacities::from_policy(
        layerfs_storage::StoragePolicy::new(1, 1_048_576, 8, 4),
    )
    .expect("capacities");
    let large_profile = CodecProfile::whole_file(&large);
    assert!(large_profile.raw_limit() > profile.raw_limit());
    assert!(large_profile.frame_limit() > profile.frame_limit());
}
