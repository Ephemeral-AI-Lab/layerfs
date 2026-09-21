//! Explicit physical format matrix: recognized framings and rejected ones.
//!
//! A reader never trial-decodes. Each framing this slice writes is recognized by
//! its version, each framing it does not implement is refused with a named
//! unsupported-format failure, and the declared allocation, frame and pack bounds
//! are enforced on the bytes rather than assumed.

mod support;

use layerfs_storage::pack::layout::{
    body_area_offset, parse_header, PackLane, PACK_MAGIC, USED_OFFSET, VERSION_NATIVE,
    VERSION_NATIVE_STORED, VERSION_ORDINARY, VERSION_POOLED_METADATA, VERSION_SINGLETON,
    VERSION_SINGLETON_STORED, VERSION_WHOLE_FILE, VERSION_WHOLE_FILE_STORED,
};
use layerfs_storage::StorageError;

/// Builds a minimal, well-formed pack of `lane` under `version`.
///
/// The control area declares `groups` groups and the length the pack assembles
/// to - the control area, the lane's whole reserved directory region and `body`
/// bytes of body - and the directory entries are left zero. A synthetic pack has
/// to carry a length its own control area agrees with, so the builder cannot
/// produce a header alone: `parse_header` reads the declared length out of the
/// bytes and refuses a pack whose bytes are not the ones it declares.
fn pack_of(lane: PackLane, version: u32, groups: u32, body: usize) -> Vec<u8> {
    let length = body_area_offset(lane) + body;
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&PACK_MAGIC);
    bytes.extend_from_slice(&version.to_le_bytes());
    bytes.extend_from_slice(&groups.to_le_bytes());
    bytes.extend_from_slice(
        &u32::try_from(length)
            .expect("synthetic pack length")
            .to_le_bytes(),
    );
    bytes.extend_from_slice(&[0, 0, 0, 0]);
    bytes.resize(length, 0);
    bytes
}

#[test]
fn every_implemented_framing_is_recognized_by_its_own_version() {
    // The three payload lanes each have two implemented versions: the one they
    // write, whose record grammar carries a stored tag, and the one they no longer
    // write but still read, which is every pack this Store wrote before the stored
    // tag existed. A version a reader does not implement is refused by name below
    // rather than trial-decoded, which is what makes the pair meaningful.
    for (version, lane) in [
        (VERSION_ORDINARY, PackLane::Ordinary),
        (VERSION_NATIVE, PackLane::Native),
        (VERSION_NATIVE_STORED, PackLane::Native),
        (VERSION_WHOLE_FILE, PackLane::WholeFile),
        (VERSION_WHOLE_FILE_STORED, PackLane::WholeFile),
        (VERSION_POOLED_METADATA, PackLane::PooledMetadata),
        (VERSION_SINGLETON, PackLane::Singleton),
        (VERSION_SINGLETON_STORED, PackLane::Singleton),
    ] {
        let bytes = pack_of(lane, version, 1, 32);
        let parsed = parse_header(&bytes).unwrap_or_else(|error| panic!("{lane:?}: {error}"));
        assert_eq!(parsed.lane, lane);
        assert_eq!(parsed.group_count, 1);
        assert_eq!(parsed.used, bytes.len(), "the declared length is the pack");
    }
}

#[test]
fn unimplemented_and_unknown_framings_are_rejected_without_a_trial_decode() {
    for version in [0_u32, 3, 5, 8, 99] {
        let bytes = pack_of(PackLane::Ordinary, version, 1, 32);
        let error = parse_header(&bytes).unwrap_err();
        assert!(
            matches!(error, StorageError::UnsupportedPolicy { field } if field == "pack framing version"),
            "version {version}: {error}"
        );
    }
    // A recognized version with a malformed control area is an integrity failure,
    // not a fallback to another framing.
    let mut wrong_magic = pack_of(PackLane::Ordinary, VERSION_ORDINARY, 1, 32);
    wrong_magic[0] = b'X';
    assert!(matches!(
        parse_header(&wrong_magic),
        Err(StorageError::Integrity(_))
    ));
    // A pack that declares more bytes than it has is refused, never short-read.
    let mut truncated = pack_of(PackLane::Ordinary, VERSION_ORDINARY, 1, 32);
    truncated.pop();
    assert!(matches!(
        parse_header(&truncated),
        Err(StorageError::Integrity("pack length"))
    ));
    // A nonzero reserved word is a framing this reader does not implement.
    let mut reserved = pack_of(PackLane::Ordinary, VERSION_ORDINARY, 1, 32);
    reserved[20] = 1;
    assert!(matches!(
        parse_header(&reserved),
        Err(StorageError::Integrity("pack reserved field"))
    ));
    // A declared length that does not even cover the reserved directory region is
    // refused rather than read as a directory.
    let mut short = pack_of(PackLane::Ordinary, VERSION_ORDINARY, 1, 0);
    let area = body_area_offset(PackLane::Ordinary);
    short.truncate(area);
    short[USED_OFFSET..USED_OFFSET + 4].copy_from_slice(&(area as u32).to_le_bytes());
    assert!(matches!(
        parse_header(&short),
        Err(StorageError::Integrity("pack directory width"))
    ));
}

#[test]
fn declared_pack_and_group_bounds_are_enforced_on_the_bytes() {
    // The ordinary lane is bounded by the 256-KiB pack limit.
    let ordinary = PackLane::Ordinary;
    let oversized = pack_of(
        ordinary,
        VERSION_ORDINARY,
        1,
        ordinary.pack_limit() - body_area_offset(ordinary) + 1,
    );
    assert!(matches!(
        parse_header(&oversized),
        Err(StorageError::Integrity("pack length"))
    ));
    // The singleton lane is bounded by its own, larger limit.
    let singleton = PackLane::Singleton;
    let oversized = pack_of(
        singleton,
        VERSION_SINGLETON,
        1,
        singleton.pack_limit() - body_area_offset(singleton) + 1,
    );
    assert!(matches!(
        parse_header(&oversized),
        Err(StorageError::Integrity("pack length"))
    ));
    // A singleton pack holds exactly one group.
    let two_groups = pack_of(PackLane::Singleton, VERSION_SINGLETON, 2, 64);
    assert!(matches!(
        parse_header(&two_groups),
        Err(StorageError::Integrity("singleton pack group count"))
    ));
    // A zero group count is not a valid directory.
    let empty = pack_of(PackLane::Native, VERSION_NATIVE, 0, 64);
    assert!(matches!(
        parse_header(&empty),
        Err(StorageError::Integrity("pack group count"))
    ));
}

#[test]
fn the_lane_table_names_one_limit_per_lane() {
    assert_eq!(PackLane::Singleton.pack_limit(), 16 * 1024 * 1024 + 4_096);
    assert_eq!(PackLane::Ordinary.pack_limit(), 1024 * 1024);
    assert_eq!(PackLane::Singleton.group_count_limit(), 1);
    assert_eq!(PackLane::PooledMetadata.body_limit(), 16 * 1024);
    for lane in PackLane::ALL {
        if lane != PackLane::Singleton {
            assert_eq!(lane.pack_limit(), 1024 * 1024, "{lane:?}");
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
/// `physical-encoding-and-packing.md` names the pooled metadata lane by its
/// framing version. The shipped candidate writes pooled **value groups** in that
/// lane and pooled **leaf** records in the Ordinary lane, distinguished by the
/// `objects.object_role` column, and it refuses v5 by design because its schema
/// identity is deliberately not the reference's. Both are recorded in that
/// document as open owner decisions; this case pins the shipped behaviour they
/// describe, and it reads the version from the lane rather than from a literal so
/// that a framing change moves one number in one place.
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

    // The persisted lanes: the value group is in the pooled lane, the leaf record
    // in the ordinary lane, and the role column says which row is which.
    let connection = rusqlite::Connection::open(&path).expect("external connection");
    let leaf_pack = support::truncate_pack(
        connection
            .query_row(
                "SELECT p.data FROM objects o JOIN object_packs p ON p.pack_id = o.pack_id \
                 WHERE o.object_id = ?1",
                [leaf_id.to_bytes().to_vec()],
                |row| row.get(0),
            )
            .expect("leaf pack"),
    );
    let group_pack = support::truncate_pack(
        connection
            .query_row(
                "SELECT p.data FROM metadata_value_groups g JOIN object_packs p \
                 ON p.pack_id = g.pack_id ORDER BY g.first_ordinal LIMIT 1",
                [],
                |row| row.get(0),
            )
            .expect("group pack"),
    );
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
        "a pooled leaf record is stored in the ordinary lane"
    );
    assert_eq!(
        VERSION_ORDINARY,
        PackLane::Ordinary.version(),
        "the ordinary lane's framing version"
    );
    assert_eq!(
        parse_header(&group_pack).expect("group header").lane,
        PackLane::PooledMetadata,
        "a pooled value group is stored in the pooled lane"
    );
    assert_eq!(
        VERSION_POOLED_METADATA,
        PackLane::PooledMetadata.version(),
        "the pooled lane's framing version"
    );
    assert_eq!(
        leaf_role,
        i64::from(ObjectRole::InodeLeaf.code()),
        "the object_role column is what distinguishes a pooled leaf from a mapping node"
    );

    // v5 is refused by name, and the refusal is the unknown-version one rather than
    // a decode attempt.
    let v5 = pack_of(PackLane::PooledMetadata, 5, 1, 32);
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

/// One ordinary-lane canonical object of exactly `canonical` bytes.
fn file_state_object(index: usize, canonical: usize) -> layerfs_content::FinalizedObject {
    let raw = support::noise(canonical - 23).as_slice().to_vec();
    let value_len = raw.len() + 10;
    let mut value = Vec::with_capacity(value_len);
    value.extend_from_slice(b"LFS5SML\0");
    value.extend_from_slice(&1_u16.to_be_bytes());
    value.extend_from_slice(&(index as u32).to_be_bytes());
    value.extend_from_slice(&raw[4..]);
    let bytes = layerfs_content::object::codec::encode_bytes_object(&value).expect("envelope");
    assert_eq!(bytes.len(), canonical, "canonical length");
    layerfs_content::FinalizedObject::new(layerfs_content::ObjectRole::FileState, bytes)
        .expect("finalized")
}

/// The partition the declared target predicts under the framing identity.
fn predicted_partition(records: usize, record_len: usize) -> Vec<usize> {
    let mut sizes = Vec::new();
    let mut current = 0_usize;
    for _ in 0..records {
        let projected =
            layerfs_storage::pack::framed_group_length(current + 1, (current + 1) * record_len)
                .expect("framing");
        if current > 0 && projected > layerfs_storage::policy::GROUP_TARGET {
            sizes.push(current);
            current = 1;
        } else {
            current += 1;
        }
    }
    if current > 0 {
        sizes.push(current);
    }
    sizes
}

/// R38: an ordinary group seals on the framed length it will actually write.
///
/// The owner used to accumulate a *per-record* framed length and add one more for
/// the arriving record, which counts a group's shared framing - one record count
/// and one end offset per record - once per record, over-counting it by `4n - 4`.
/// Groups therefore sealed early and the physical layout disagreed with the
/// declared target. This case derives the expected partition from the published
/// framing identity and the declared target, compares it with the partition the
/// store actually recorded, and checks every group's decoded body length against
/// the same identity. The over-counting form produces `[11, 11, 2]` here where the
/// identity predicts `[12, 12]`.
#[test]
fn an_ordinary_group_seals_on_the_framed_length_it_writes() {
    const RECORDS: usize = 24;
    const CANONICAL: usize = 4_090;
    // The ordinary lane stores a FULL record: one tag byte plus the canonical bytes.
    let record_len = CANONICAL + 1;

    let dir = support::TempDir::new("group-seal");
    let path = dir.store_path("group-seal");
    let store = support::create_store(&path);
    let objects = (0..RECORDS)
        .map(|index| file_state_object(index, CANONICAL))
        .collect::<Vec<_>>();
    let outcome = support::disabled(|scope| {
        let mut operation = store.begin_save(scope.child("storage.begin"))?;
        for object in objects {
            operation.accept(object)?;
        }
        operation.finish(scope.child("storage.finish"))
    })
    .expect("save");
    assert_eq!(outcome.inserted, RECORDS as u64);
    drop(store);

    let connection = rusqlite::Connection::open(&path).expect("external connection");
    let stores = {
        let mut statement = connection
            .prepare("SELECT pack_id, group_number, COUNT(*) FROM objects GROUP BY pack_id, group_number ORDER BY pack_id, group_number")
            .expect("location query");
        statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                ))
            })
            .expect("rows")
            .collect::<Result<Vec<_>, _>>()
            .expect("collect")
    };
    let sizes = stores
        .iter()
        .map(|(_, _, count)| *count as usize)
        .collect::<Vec<_>>();
    let expected = predicted_partition(RECORDS, record_len);
    assert_eq!(
        sizes, expected,
        "the recorded partition is not the one the framing identity and the declared target predict"
    );
    assert_eq!(
        expected,
        vec![12, 12],
        "the discriminating partition changed"
    );

    // The identity is not just arithmetic: every group's decoded body length, read
    // from the pack the store wrote, is exactly the projected framed length.
    let mut checked = 0;
    let mut bodies: std::collections::BTreeMap<i64, Vec<u8>> = std::collections::BTreeMap::new();
    for (pack_id, group_number, count) in &stores {
        let data = bodies
            .entry(*pack_id)
            .or_insert_with(|| support::read_pack_row(&connection, *pack_id));
        let header = parse_header(data).expect("pack header");
        let view = layerfs_storage::pack::group_view(data, header, *group_number as usize)
            .expect("group view");
        let projected = layerfs_storage::pack::framed_group_length(
            *count as usize,
            *count as usize * record_len,
        )
        .expect("framing");
        assert_eq!(
            view.decoded_length, projected,
            "pack {pack_id} group {group_number} framed length"
        );
        assert!(view.decoded_length <= layerfs_storage::policy::GROUP_TARGET);
        checked += 1;
    }
    assert_eq!(
        checked,
        expected.len(),
        "every group was inspected exactly once"
    );
}
