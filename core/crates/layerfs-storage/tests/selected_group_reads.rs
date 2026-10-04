//! Whole/range policy, exact output and corruption refusal through ordinary C2.
#[path = "support/memory_metadata.rs"]
mod metadata;
use layerfs_content::{FinalizedObject, ObjectRole};
use layerfs_storage::{
    location::{ObjectLocation, PackDomain, PackInfo},
    pack::{
        assemble, build_group,
        layout::{self, PackLane},
    },
    port::*,
    Storage,
};
use std::sync::Arc;

fn fixture() -> (Arc<metadata::MemoryMetadata>, Vec<FinalizedObject>, usize) {
    let metadata = Arc::new(metadata::MemoryMetadata::default());
    let mut random = 931_823u64;
    let objects: Vec<_> = (0..8)
        .map(|_| {
            let bytes: Vec<_> = (0..9000)
                .map(|_| {
                    random ^= random << 13;
                    random ^= random >> 7;
                    random ^= random << 17;
                    random as u8
                })
                .collect();
            FinalizedObject::new(
                ObjectRole::FileState,
                layerfs_content::object::codec::encode_bytes_object(&bytes).unwrap(),
            )
            .unwrap()
        })
        .collect();
    let groups: Vec<_> = objects
        .iter()
        .map(|object| {
            let mut record = vec![0];
            record.extend_from_slice(object.canonical());
            build_group(PackLane::Ordinary, &[record], None).unwrap()
        })
        .collect();
    let body = assemble(PackLane::Ordinary, &groups).unwrap();
    let length = body.len();
    metadata
        .publish(&Publication {
            packs: vec![PublishedPack {
                info: PackInfo {
                    pack_id: 1,
                    domain: PackDomain::Metadata,
                    length,
                    key: ObjectKey::for_bytes(&body),
                },
                body: Arc::new(body),
            }],
            objects: objects
                .iter()
                .enumerate()
                .map(|(number, object)| ObjectLocation {
                    object_id: object.id(),
                    role: object.role(),
                    canonical_length: object.canonical_len(),
                    pack_id: 1,
                    group_number: number,
                    record_number: 0,
                })
                .collect(),
            ..Publication::default()
        })
        .unwrap();
    (metadata, objects, length)
}
#[test]
fn adjacent_sparse_groups_coalesce_and_dense_promotion_preserves_bytes_and_order() {
    let (metadata, objects, length) = fixture();
    let storage = Storage::new(metadata.clone()).unwrap();
    let reader = storage.reader().unwrap();
    let sparse = [1, 0, 1];
    assert_eq!(
        reader
            .read_objects(&sparse.map(|i| objects[i].id()))
            .unwrap(),
        sparse.map(|i| objects[i].canonical().to_vec())
    );
    let work = storage.diagnostics();
    assert_eq!(work.range_selected, 1);
    assert_eq!(work.range_scan_bytes, 0);
    assert!(work.range_acquired_bytes < length as u64 / 2);
    assert!(work.range_materialized_bytes < length as u64);
    reader.read_objects(&[objects[1].id()]).unwrap();
    assert_eq!(storage.diagnostics().range_selected, 1);
    assert_eq!(
        reader
            .read_objects(&objects.iter().map(FinalizedObject::id).collect::<Vec<_>>())
            .unwrap(),
        objects
            .iter()
            .map(|object| object.canonical().to_vec())
            .collect::<Vec<_>>()
    );
    assert_eq!(storage.diagnostics().whole_due_density, 1);
    assert_eq!(storage.diagnostics().read_pack_selections, 2);
}
#[test]
fn new_sparse_group_miss_promotes_retained_pack_then_all_siblings_reuse() {
    let (metadata, objects, _) = fixture();
    let storage = Storage::new(metadata).unwrap();
    let reader = storage.reader().unwrap();
    for (step, number) in [0, 0, 7, 3, 5, 1, 6, 2, 4].into_iter().enumerate() {
        assert_eq!(
            reader.read_objects(&[objects[number].id()]).unwrap()[0],
            objects[number].canonical()
        );
        let work = storage.diagnostics();
        assert_eq!(work.range_selected, 1);
        assert_eq!(work.whole_due_reuse, u64::from(step >= 2));
        assert_eq!(work.read_pack_selections, if step < 2 { 1 } else { 2 });
    }
}
#[test]
fn random_sparse_groups_preserve_duplicate_slots() {
    let (metadata, objects, _) = fixture();
    let storage = Storage::new(metadata).unwrap();
    let wanted = [7, 3, 7, 0];
    assert_eq!(
        storage
            .reader()
            .unwrap()
            .read_objects(&wanted.map(|i| objects[i].id()))
            .unwrap(),
        wanted.map(|i| objects[i].canonical().to_vec())
    );
    assert_eq!(storage.diagnostics().range_selected, 1);
}
#[test]
fn unread_corruption_is_outside_scoped_read_but_whole_pack_audit_refuses_it() {
    let (metadata, objects, _) = fixture();
    {
        let mut state = metadata.state.lock().unwrap();
        let pack = state.packs.get_mut(&1).unwrap();
        let last = pack.body.len() - 1;
        Arc::make_mut(&mut pack.body)[last] ^= 1;
    }
    let storage = Storage::new(metadata.clone()).unwrap();
    assert_eq!(
        storage
            .reader()
            .unwrap()
            .read_objects(&[objects[0].id()])
            .unwrap()[0],
        objects[0].canonical()
    );
    assert!(metadata.read_packs(&[1], &mut Vec::new()).is_err());
}
#[test]
fn accessed_record_corruption_fails_canonical_authentication() {
    let (metadata, objects, _) = fixture();
    {
        let mut state = metadata.state.lock().unwrap();
        let pack = state.packs.get_mut(&1).unwrap();
        let header = layout::parse_directory_header(&pack.body, pack.info.length).unwrap();
        let view = layout::directory_group_views(&pack.body, header).unwrap()[0];
        Arc::make_mut(&mut pack.body)[view.end - 1] ^= 1;
    }
    let store = Storage::new(metadata).unwrap();
    assert!(store
        .reader()
        .unwrap()
        .read_objects(&[objects[0].id()])
        .is_err());
}
#[test]
fn hash_valid_malformed_directory_is_refused_by_the_shared_prefix_validator() {
    let (metadata, objects, _) = fixture();
    {
        let mut state = metadata.state.lock().unwrap();
        let pack = state.packs.get_mut(&1).unwrap();
        Arc::make_mut(&mut pack.body)[layout::HEADER_LEN..layout::HEADER_LEN + 4]
            .copy_from_slice(&1u32.to_le_bytes());
        pack.info.key = ObjectKey::for_bytes(&pack.body);
    }
    let storage = Storage::new(metadata).unwrap();
    assert!(storage
        .reader()
        .unwrap()
        .read_objects(&[objects[0].id()])
        .is_err());
}

#[test]
fn a_sparse_prefix_target_reconstructs_its_required_base_exactly() {
    use layerfs_storage::{
        encoding::{encode_full, encode_prefix, raw_payload, CompressionWorkspace},
        policy::{StorageCapacities, StoragePolicy},
        save::SaveProfile,
    };
    let metadata = Arc::new(metadata::MemoryMetadata::default());
    let capacities = StorageCapacities::from_policy(StoragePolicy::frozen_default()).unwrap();
    let mut random = 763_291u64;
    let base: Vec<_> = (0..32_000)
        .map(|_| {
            random ^= random << 13;
            random ^= random >> 7;
            random ^= random << 17;
            random as u8
        })
        .collect();
    let mut target = base.clone();
    target[16_000] ^= 1;
    let objects: Vec<_> = [base, target]
        .into_iter()
        .map(|raw| {
            FinalizedObject::new(
                ObjectRole::WholeFile,
                layerfs_content::encode_whole_file_payload(&raw).unwrap(),
            )
            .unwrap()
        })
        .collect();
    let mut workspace = CompressionWorkspace::new().unwrap();
    let mut profile = SaveProfile::default();
    let full = encode_full(
        objects[0].canonical(),
        ObjectRole::WholeFile,
        &capacities,
        &mut workspace,
        &mut profile,
    )
    .unwrap();
    let prefix = encode_prefix(
        objects[1].canonical(),
        ObjectRole::WholeFile,
        objects[0].id(),
        raw_payload(objects[0].canonical(), ObjectRole::WholeFile).unwrap(),
        &capacities,
        &mut workspace,
        &mut profile,
    )
    .unwrap();
    let groups = [full, prefix]
        .into_iter()
        .map(|record| build_group(PackLane::WholeFile, &[record.record], None).unwrap())
        .collect::<Vec<_>>();
    let body = assemble(PackLane::WholeFile, &groups).unwrap();
    metadata
        .publish(&Publication {
            packs: vec![PublishedPack {
                info: PackInfo {
                    pack_id: 1,
                    domain: PackDomain::Payload,
                    length: body.len(),
                    key: ObjectKey::for_bytes(&body),
                },
                body: Arc::new(body),
            }],
            objects: objects
                .iter()
                .enumerate()
                .map(|(number, object)| ObjectLocation {
                    object_id: object.id(),
                    role: object.role(),
                    canonical_length: object.canonical_len(),
                    pack_id: 1,
                    group_number: number,
                    record_number: 0,
                })
                .collect(),
            ..Publication::default()
        })
        .unwrap();
    let storage = Storage::new(metadata).unwrap();
    assert_eq!(
        storage
            .reader()
            .unwrap()
            .read_objects(&[objects[1].id()])
            .unwrap()[0],
        objects[1].canonical()
    );
    assert_eq!(storage.diagnostics().range_selected, 1);
    assert_eq!(storage.diagnostics().whole_due_density, 1);
}

fn native_fixture(packs: usize) -> (Arc<metadata::MemoryMetadata>, Vec<FinalizedObject>) {
    use layerfs_storage::{
        encoding::{encode_full, CompressionWorkspace},
        policy::{StorageCapacities, StoragePolicy},
        save::SaveProfile,
    };
    let metadata = Arc::new(metadata::MemoryMetadata::default());
    let capacities = StorageCapacities::from_policy(StoragePolicy::frozen_default()).unwrap();
    let mut random = 793_811u64;
    let objects: Vec<_> = (0..packs * 8)
        .map(|_| {
            let bytes: Vec<_> = (0..32_000)
                .map(|_| {
                    random ^= random << 13;
                    random ^= random >> 7;
                    random ^= random << 17;
                    random as u8
                })
                .collect();
            FinalizedObject::new(
                ObjectRole::Chunk,
                layerfs_content::file::mapping::encode_chunk_object(&bytes).unwrap(),
            )
            .unwrap()
        })
        .collect();
    let mut workspace = CompressionWorkspace::new().unwrap();
    let mut profile = SaveProfile::default();
    let mut published = Vec::new();
    for (number, pack_objects) in objects.chunks(8).enumerate() {
        let groups = pack_objects
            .iter()
            .map(|object| {
                let record = encode_full(
                    object.canonical(),
                    ObjectRole::Chunk,
                    &capacities,
                    &mut workspace,
                    &mut profile,
                )
                .unwrap();
                build_group(PackLane::Native, &[record.record], None).unwrap()
            })
            .collect::<Vec<_>>();
        let body = assemble(PackLane::Native, &groups).unwrap();
        published.push(PublishedPack {
            info: PackInfo {
                pack_id: number as i64 + 1,
                domain: PackDomain::Payload,
                length: body.len(),
                key: ObjectKey::for_bytes(&body),
            },
            body: Arc::new(body),
        });
    }
    metadata
        .publish(&Publication {
            packs: published,
            objects: objects
                .iter()
                .enumerate()
                .map(|(number, object)| ObjectLocation {
                    object_id: object.id(),
                    role: object.role(),
                    canonical_length: object.canonical_len(),
                    pack_id: (number / 8) as i64 + 1,
                    group_number: number % 8,
                    record_number: 0,
                })
                .collect(),
            ..Publication::default()
        })
        .unwrap();
    (metadata, objects)
}
#[test]
fn sparse_native_records_reconstruct_canonical_chunk_bytes() {
    let (metadata, objects) = native_fixture(1);
    let storage = Storage::new(metadata).unwrap();
    assert_eq!(
        storage
            .reader()
            .unwrap()
            .read_objects(&[objects[7].id()])
            .unwrap()[0],
        objects[7].canonical()
    );
    assert_eq!(storage.diagnostics().range_selected, 1);
}

#[test]
fn sparse_payload_siblings_survive_multiple_pack_pressure_without_whole_promotion() {
    let packs = 9;
    let (metadata, objects) = native_fixture(packs);
    let storage = Storage::new(metadata).unwrap();
    let reader = storage.reader().unwrap();
    for group in [0, 7, 0] {
        for pack in 0..packs {
            let object = &objects[pack * 8 + group];
            assert_eq!(
                reader.read_objects(&[object.id()]).unwrap()[0],
                object.canonical()
            );
        }
    }
    assert_eq!(
        storage.diagnostics().read_pack_selections,
        (packs * 2) as u64
    );
    assert_eq!(storage.diagnostics().pack_evictions, 0);
}

#[test]
fn dense_payload_coverage_promotes_once_and_then_reuses_sibling_groups() {
    let (metadata, objects) = native_fixture(1);
    let storage = Storage::new(metadata).unwrap();
    let reader = storage.reader().unwrap();
    for object in &objects {
        assert_eq!(
            reader.read_objects(&[object.id()]).unwrap()[0],
            object.canonical()
        );
    }
    let before = storage.diagnostics();
    assert!(before.range_selected > 1);
    assert_eq!(before.whole_due_reuse, 1);
    assert_eq!(
        reader.read_objects(&[objects[0].id()]).unwrap()[0],
        objects[0].canonical()
    );
    assert_eq!(
        storage.diagnostics().read_pack_selections,
        before.read_pack_selections
    );
}
