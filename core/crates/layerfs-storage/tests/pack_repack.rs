//! `pack::repack::retain_records` rebuilds a sealed pack from the records its
//! caller keeps: every kept record is byte-identical at its new ordinal, a
//! group that keeps everything is carried over as stored, a group that keeps
//! nothing is left out, and a kept set the pack cannot satisfy is refused.
use layerfs_content::file::mapping::{self, ExtentNode, ExtentSlice};
use layerfs_content::{encode_whole_file_payload, FinalizedObject, ObjectId, ObjectRole};
use layerfs_storage::encoding::{
    encode_full, framed_record, CompressionWorkspace, DecompressionWorkspace,
};
use layerfs_storage::pack::audit::validate_records;
use layerfs_storage::pack::layout::{group_view, parse_header, GroupCodec, PackLane};
use layerfs_storage::pack::repack::{record_counts, retain_records};
use layerfs_storage::pack::{assemble, build_group};
use layerfs_storage::save::SaveProfile;
use layerfs_storage::{StorageCapacities, StoragePolicy};

/// Deterministic bytes that do not compress.
fn noise(seed: u64, len: usize) -> Vec<u8> {
    let mut state = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
    let mut out = Vec::with_capacity(len + 8);
    while out.len() < len {
        state ^= state >> 12;
        state ^= state << 25;
        state ^= state >> 27;
        out.extend_from_slice(&state.wrapping_mul(0x2545_F491_4F6C_DD1D).to_le_bytes());
    }
    out.truncate(len);
    out
}

fn object(lane: PackLane, seed: u64) -> FinalizedObject {
    match lane {
        PackLane::WholeFile | PackLane::Singleton => FinalizedObject::new(
            ObjectRole::WholeFile,
            encode_whole_file_payload(&noise(seed, 700 + 13 * seed as usize)).unwrap(),
        ),
        PackLane::Chunk => FinalizedObject::new(
            ObjectRole::Chunk,
            mapping::encode_chunk_object(&noise(seed, 900 + 7 * seed as usize)).unwrap(),
        ),
        // An extent leaf of many slices of one chunk: a metadata page that
        // compresses, like the ones a real tree stores.
        PackLane::Ordinary => FinalizedObject::new(
            ObjectRole::ExtentLeaf,
            mapping::encode_node(
                &ExtentNode::Leaf {
                    subtree_logical_bytes: 64 * 20,
                    extents: (0..20)
                        .map(|_| {
                            ExtentSlice::new(ObjectId::for_bytes(&seed.to_le_bytes()), 0, 64)
                                .unwrap()
                        })
                        .collect(),
                },
                true,
            )
            .unwrap(),
        ),
    }
    .unwrap()
}

/// One pack of `lane` whose groups hold `shape[g]` records each, and every
/// record exactly as it was framed.
fn pack(lane: PackLane, shape: &[usize]) -> (Vec<u8>, Vec<Vec<Vec<u8>>>) {
    let capacities = StorageCapacities::from_policy(StoragePolicy::frozen_default()).unwrap();
    let mut workspace = CompressionWorkspace::new().unwrap();
    let mut profile = SaveProfile::default();
    let mut seed = 1u64;
    let mut records = Vec::new();
    let mut groups = Vec::new();
    for count in shape {
        let mut group = Vec::new();
        for _ in 0..*count {
            let object = object(lane, seed);
            seed += 1;
            let encoded = encode_full(
                object.canonical(),
                object.role(),
                &capacities,
                &mut workspace,
                &mut profile,
            )
            .unwrap();
            // A singleton pack frames the record a whole-file encoding emits.
            if lane != PackLane::Singleton {
                assert_eq!(encoded.lane, lane);
            }
            group.push(encoded.record);
        }
        groups.push(build_group(lane, &group, Some(&mut workspace)).unwrap());
        records.push(group);
    }
    (assemble(lane, &groups).unwrap(), records)
}

/// The decoded records of every group of a pack, as the pack stores them.
fn stored(body: &[u8]) -> Vec<Vec<Vec<u8>>> {
    let header = parse_header(body).unwrap();
    let mut workspace = DecompressionWorkspace::new().unwrap();
    (0..header.group_count)
        .map(|number| {
            let view = group_view(body, header, number).unwrap();
            let bytes = &body[view.start..view.end];
            let group = match view.codec {
                GroupCodec::Raw => bytes.to_vec(),
                GroupCodec::Zstandard => workspace
                    .decompress_group(bytes, view.decoded_length)
                    .unwrap(),
            };
            let mut records = Vec::new();
            while let Ok(record) = framed_record(&group, records.len()) {
                records.push(record.to_vec());
            }
            records
        })
        .collect()
}

fn retain(body: &[u8], keep: &[Vec<usize>]) -> Option<layerfs_storage::pack::repack::Repacked> {
    retain_records(
        body,
        keep,
        &mut CompressionWorkspace::new().unwrap(),
        &mut DecompressionWorkspace::new().unwrap(),
    )
    .unwrap()
}

#[test]
fn kept_records_keep_their_bytes_at_their_new_ordinals_in_every_lane() {
    for lane in [PackLane::WholeFile, PackLane::Chunk, PackLane::Ordinary] {
        let (body, _) = pack(lane, &[5, 3, 4, 2]);
        let before = stored(&body);
        let keep = vec![vec![1, 3, 4], vec![], vec![0, 1, 2, 3], vec![1]];
        let repacked = retain(&body, &keep).expect("records were dropped");
        assert_eq!(
            repacked.groups,
            vec![Some(0), None, Some(1), Some(2)],
            "{lane:?}: the emptied group is left out and later groups move up"
        );
        assert_eq!(repacked.records, 8);
        assert!(repacked.body.len() < body.len(), "{lane:?} shrank");
        assert_eq!(
            validate_records(&repacked.body, &mut || true).unwrap(),
            8,
            "{lane:?}: the rebuilt pack is a well-formed pack of the kept records"
        );
        let header = parse_header(&repacked.body).unwrap();
        assert_eq!(header.lane, lane);
        assert_eq!(header.group_count, 3);
        let after = stored(&repacked.body);
        for (group, kept) in keep.iter().enumerate() {
            let Some(moved) = repacked.groups[group] else {
                continue;
            };
            assert_eq!(after[moved].len(), kept.len());
            for (position, record) in kept.iter().enumerate() {
                assert_eq!(
                    after[moved][position], before[group][*record],
                    "{lane:?} group {group} record {record}"
                );
            }
        }
    }
}

#[test]
fn a_group_that_keeps_every_record_is_carried_over_as_stored() {
    for lane in [PackLane::WholeFile, PackLane::Chunk, PackLane::Ordinary] {
        let (body, _) = pack(lane, &[3, 6]);
        let old = parse_header(&body).unwrap();
        let kept_view = group_view(&body, old, 1).unwrap();
        let repacked = retain(&body, &[vec![], vec![0, 1, 2, 3, 4, 5]]).unwrap();
        let new = parse_header(&repacked.body).unwrap();
        let view = group_view(&repacked.body, new, 0).unwrap();
        assert_eq!(view.codec, kept_view.codec);
        assert_eq!(
            &repacked.body[view.start..view.end],
            &body[kept_view.start..kept_view.end],
            "{lane:?}: the stored group bytes are unchanged"
        );
    }
}

#[test]
fn a_compressed_group_is_decoded_and_built_again() {
    let (body, records) = pack(PackLane::Ordinary, &[12]);
    let header = parse_header(&body).unwrap();
    assert_eq!(
        group_view(&body, header, 0).unwrap().codec,
        GroupCodec::Zstandard,
        "the fixture group is stored compressed"
    );
    let keep = vec![vec![0, 2, 4, 6, 8, 10]];
    let repacked = retain(&body, &keep).unwrap();
    let after = stored(&repacked.body);
    assert_eq!(after.len(), 1);
    for (position, record) in keep[0].iter().enumerate() {
        assert_eq!(after[0][position], records[0][*record]);
    }
    let rebuilt = parse_header(&repacked.body).unwrap();
    assert_eq!(
        group_view(&repacked.body, rebuilt, 0).unwrap().codec,
        GroupCodec::Zstandard,
        "six similar pages still compress"
    );
}

#[test]
fn a_pack_that_already_holds_exactly_the_kept_records_is_left_alone() {
    for lane in [PackLane::WholeFile, PackLane::Chunk, PackLane::Ordinary] {
        let (body, _) = pack(lane, &[2, 3]);
        assert!(retain(&body, &[vec![0, 1], vec![0, 1, 2]]).is_none());
    }
    let (singleton, _) = pack(PackLane::Singleton, &[1]);
    assert!(
        retain(&singleton, &[vec![0]]).is_none(),
        "a singleton pack has nothing to drop"
    );
}

#[test]
fn a_kept_set_the_pack_cannot_satisfy_is_refused() {
    let (body, _) = pack(PackLane::WholeFile, &[3, 2]);
    let refuse = |keep: &[Vec<usize>]| {
        retain_records(
            &body,
            keep,
            &mut CompressionWorkspace::new().unwrap(),
            &mut DecompressionWorkspace::new().unwrap(),
        )
        .is_err()
    };
    assert!(refuse(&[vec![0, 3], vec![0]]), "a record past the group");
    assert!(refuse(&[vec![1, 0], vec![0]]), "records out of order");
    assert!(refuse(&[vec![1, 1], vec![0]]), "a record named twice");
    assert!(refuse(&[vec![], vec![]]), "no record kept at all");
    assert!(refuse(&[vec![0]]), "a group left unnamed");
    assert!(
        refuse(&[vec![0], vec![0], vec![0]]),
        "a group the pack lacks"
    );
}

#[test]
fn record_counts_are_read_from_the_pack_itself_in_every_lane() {
    let mut workspace = DecompressionWorkspace::new().unwrap();
    for lane in [PackLane::WholeFile, PackLane::Chunk, PackLane::Ordinary] {
        let (body, _) = pack(lane, &[5, 3, 12, 2]);
        assert_eq!(
            record_counts(&body, &mut workspace).unwrap(),
            vec![5, 3, 12, 2],
            "{lane:?}"
        );
        let repacked = retain(&body, &[vec![1, 3, 4], vec![], vec![0, 5, 11], vec![1]]).unwrap();
        assert_eq!(
            record_counts(&repacked.body, &mut workspace).unwrap(),
            vec![3, 3, 1],
            "{lane:?}: the rebuilt pack states the kept records, group by group"
        );
    }
    let (compressed, _) = pack(PackLane::Ordinary, &[12]);
    let header = parse_header(&compressed).unwrap();
    assert_eq!(
        group_view(&compressed, header, 0).unwrap().codec,
        GroupCodec::Zstandard
    );
    assert_eq!(
        record_counts(&compressed, &mut workspace).unwrap(),
        vec![12]
    );
    let (singleton, _) = pack(PackLane::Singleton, &[1]);
    assert_eq!(record_counts(&singleton, &mut workspace).unwrap(), vec![1]);
    // A pack cut short is refused, not counted.
    assert!(record_counts(&compressed[..compressed.len() - 1], &mut workspace).is_err());
}
