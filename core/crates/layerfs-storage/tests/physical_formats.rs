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

/// The two recorded lane/scope deviations, asserted so they cannot drift silently.
///
/// `physical-encoding-and-packing.md` names v6 as the active pooled metadata lane.
/// The shipped candidate writes pooled **value groups** in v6 and pooled **leaf**
/// records in the v1 Ordinary lane, distinguished by the `objects.object_role`
/// column, and it refuses v5 by design because its schema identity is deliberately
/// not the reference's. Both are recorded in that document as open owner
/// decisions; this case pins the shipped behaviour they describe.
#[test]
fn the_pooled_lane_assignment_and_the_v5_scope_are_the_shipped_ones() {
    use layerfs_content::inode_leaf::{
        encode_inode_value, InodeKind, InodeLeaf, InodeLeafRow, InodeValue, INODE_VALUE_BYTES,
        LEAF_ROW_BYTES,
    };
    use layerfs_content::{FinalizedObject, ObjectId, ObjectRole};
    use support::{create_store, save_one, TempDir};

    let dir = TempDir::new("formats-pooled");
    let path = dir.store_path("formats");
    let store = create_store(&path);
    let mut seed = [0_u8; 32];
    seed[..8].copy_from_slice(&7_u64.to_be_bytes());
    let value: [u8; INODE_VALUE_BYTES] = encode_inode_value(InodeValue {
        kind: InodeKind::RegularFile,
        namespace_ref_count: 1,
        content_root: ObjectId::for_bytes(&seed),
        metadata_root: ObjectId::for_bytes(&[7_u8; 8]),
    });
    let canonical = InodeLeaf {
        subtree_bytes: LEAF_ROW_BYTES as u64,
        rows: vec![InodeLeafRow { serial: 1, value }],
    }
    .encode()
    .expect("canonical leaf");
    let leaf = FinalizedObject::new(ObjectRole::InodeLeaf, canonical).expect("finalized");
    let leaf_id = leaf.id();
    save_one(&store, leaf).expect("pooled save");
    drop(store);

    // The persisted lanes: the value group is in the v6 pooled lane, the leaf
    // record in the v1 ordinary lane, and the role column says which row is which.
    let connection = rusqlite::Connection::open(&path).expect("external connection");
    let leaf_pack: Vec<u8> = connection
        .query_row(
            "SELECT p.data FROM objects o JOIN object_packs p ON p.pack_id = o.pack_id \
             WHERE o.object_id = ?1",
            [leaf_id.to_bytes().to_vec()],
            |row| row.get(0),
        )
        .expect("leaf pack");
    let group_pack: Vec<u8> = connection
        .query_row(
            "SELECT p.data FROM metadata_value_groups g JOIN object_packs p \
             ON p.pack_id = g.pack_id ORDER BY g.first_ordinal LIMIT 1",
            [],
            |row| row.get(0),
        )
        .expect("group pack");
    let leaf_role: i64 = connection
        .query_row(
            "SELECT object_role FROM objects WHERE object_id = ?1",
            [leaf_id.to_bytes().to_vec()],
            |row| row.get(0),
        )
        .expect("role column");
    drop(connection);
    assert_eq!(
        parse_header(&leaf_pack).expect("leaf header").lane,
        PackLane::Ordinary,
        "a pooled leaf record is stored in the v1 ordinary lane"
    );
    assert_eq!(
        VERSION_ORDINARY,
        PackLane::Ordinary.version(),
        "v1 is the ordinary lane"
    );
    assert_eq!(
        parse_header(&group_pack).expect("group header").lane,
        PackLane::PooledMetadata,
        "a pooled value group is stored in the v6 pooled lane"
    );
    assert_eq!(VERSION_POOLED_METADATA, 6, "the pooled lane is v6");
    assert_eq!(
        leaf_role,
        i64::from(ObjectRole::InodeLeaf.code()),
        "the object_role column is what distinguishes a pooled leaf from a mapping node"
    );

    // v5 is refused by name, and the refusal is the unknown-version one rather than
    // a decode attempt.
    let v5 = header(5, 1, 16 + 16 + 32);
    let error = parse_header(&v5).unwrap_err();
    assert!(
        matches!(error, StorageError::UnsupportedPolicy { field } if field == "pack framing version"),
        "v5 must be refused as an unsupported framing: {error}"
    );
}

#[test]
fn closing_a_pack_and_retaining_its_tail_assemble_the_same_bytes() {
    // Placement closes a full pack by handing its groups to the consuming
    // assembly, which releases each body as it copies it, and keeps the tail when
    // the pack stays open. The two paths must produce byte-identical packs: the
    // difference is when the constituent bodies are released, never what is
    // written.
    use layerfs_storage::encoding::CompressionWorkspace;
    use layerfs_storage::pack::{assemble, assemble_consuming, build_group};

    let mut workspace = CompressionWorkspace::new().expect("encode workspace");
    for lane in [
        PackLane::Ordinary,
        PackLane::Native,
        PackLane::WholeFile,
        PackLane::PooledMetadata,
        PackLane::Singleton,
    ] {
        let groups = match lane {
            PackLane::WholeFile => {
                vec![build_group(lane, &[vec![0x11; 4_096]], None).expect("compact group")]
            }
            PackLane::PooledMetadata => vec![
                build_group(lane, &[vec![0x22; 512]], None).expect("pooled group"),
                build_group(lane, &[vec![0x33; 256]], None).expect("pooled group"),
            ],
            PackLane::Singleton => {
                vec![build_group(lane, &[vec![0x44; 32_768]], None).expect("singleton group")]
            }
            _ => vec![
                build_group(
                    lane,
                    &[vec![0x55; 300], vec![0x66; 200]],
                    Some(&mut workspace),
                )
                .expect("ordinary group"),
                build_group(lane, &[vec![0x77; 1_024]], Some(&mut workspace))
                    .expect("ordinary group"),
                build_group(lane, &[vec![0x88; 4_096]], Some(&mut workspace))
                    .expect("ordinary group"),
            ],
        };
        let borrowed = assemble(lane, &groups).expect("retained-tail assembly");
        let consumed = assemble_consuming(lane, groups).expect("consuming assembly");
        assert_eq!(borrowed, consumed, "{lane:?}");
    }
}
