//! Unchanged pooled and filesystem vectors through reference-closed port saves.
#[path = "support/memory_engines.rs"]
mod engines;
mod support;
use engines::{MemoryMetadata, MemoryObjects};
use layerfs_content::{
    inode_leaf::{
        encode_inode_value, InodeKind, InodeLeaf, InodeLeafRow, InodeValue, LEAF_ROW_BYTES,
    },
    AdvisoryPredecessors, FinalizedObject, ObjectId, ObjectRole,
};
use layerfs_storage::{pack::PackLane, port::*, Storage, StorageError};
use std::sync::Arc;
fn value(seed: u64) -> [u8; 73] {
    encode_inode_value(InodeValue {
        kind: InodeKind::RegularFile,
        namespace_ref_count: 1,
        content_root: ObjectId::for_bytes(&seed.to_be_bytes()),
        metadata_root: ObjectId::for_bytes(&seed.to_le_bytes()),
    })
}
fn leaf(serial: u64, values: &[[u8; 73]]) -> FinalizedObject {
    let rows = values
        .iter()
        .enumerate()
        .map(|(i, v)| InodeLeafRow {
            serial: serial + i as u64,
            value: *v,
        })
        .collect::<Vec<_>>();
    FinalizedObject::new(
        ObjectRole::InodeLeaf,
        InodeLeaf {
            subtree_bytes: rows.len() as u64 * LEAF_ROW_BYTES as u64,
            rows,
        }
        .encode()
        .unwrap(),
    )
    .unwrap()
}
fn setup() -> (Arc<MemoryMetadata>, Arc<MemoryObjects>, Storage) {
    let metadata = Arc::new(MemoryMetadata::default());
    let objects = Arc::new(MemoryObjects::default());
    let storage = Storage::new(metadata.clone(), objects.clone()).unwrap();
    (metadata, objects, storage)
}
fn save(storage: &Storage, objects: Vec<FinalizedObject>) -> layerfs_storage::WriteOutcome {
    let operation = storage.begin_save().unwrap();
    for object in objects {
        operation.accept(object).unwrap();
    }
    operation.finish().unwrap()
}
fn packs(metadata: &MemoryMetadata) -> Vec<Vec<u8>> {
    let mut packs: Vec<_> = metadata
        .state
        .lock()
        .unwrap()
        .packs
        .values()
        .map(|p| p.body.clone().unwrap())
        .collect();
    packs.sort();
    packs
}
fn old_packs(path: &std::path::Path) -> Vec<Vec<u8>> {
    let (batch, payloads) = engines::snapshot(path);
    assert!(payloads.is_empty());
    let mut out: Vec<_> = batch.packs.into_iter().map(|p| p.body.unwrap()).collect();
    out.sort();
    out
}
#[test]
fn a_real_hundred_row_leaf_has_the_same_canonical_and_sealed_bytes() {
    let (metadata, objects, storage) = setup();
    let object = leaf(1, &(0..100).map(value).collect::<Vec<_>>());
    let dir = support::TempDir::new("port-pool-single");
    let path = dir.store_path("old");
    let old = support::create_store(&path);
    support::save_one(&old, object.clone()).unwrap();
    let result = save(&storage, vec![object.clone()]);
    assert_eq!(result.inserted, 1);
    assert_eq!(result.pool.new_values, 100);
    assert_eq!(result.pool.full_leaves, 1);
    assert_eq!(
        storage
            .reader()
            .unwrap()
            .read_objects(&[object.id()])
            .unwrap(),
        vec![object.canonical().to_vec()]
    );
    assert_eq!(packs(&metadata), old_packs(&path));
    assert!(objects.calls.lock().unwrap().is_empty());
    let counts = storage.diagnostics();
    assert_eq!(counts.pooled_packs, 1);
    assert_eq!(counts.pooled_groups, 1);
    assert_eq!(
        counts.reserved_directory_bytes,
        layerfs_storage::pack::directory_capacity(PackLane::PooledMetadata) as u64
    );
    println!("DIAGNOSTIC pooled-single {:?}", counts);
}
#[test]
fn same_save_and_reopened_saves_reuse_exact_value_ordinals() {
    let (metadata, objects, storage) = setup();
    let values = [value(1), value(2), value(1)];
    let first = leaf(1, &values);
    let second = leaf(20, &values);
    save(&storage, vec![first.clone(), second.clone()]);
    assert_eq!(
        metadata
            .state
            .lock()
            .unwrap()
            .groups
            .values()
            .map(|r| r.count)
            .sum::<usize>(),
        2
    );
    let reopened = Storage::new(metadata.clone(), objects).unwrap();
    let third = leaf(50, &values);
    save(&reopened, vec![third.clone()]);
    assert_eq!(
        metadata
            .state
            .lock()
            .unwrap()
            .groups
            .values()
            .map(|r| r.count)
            .sum::<usize>(),
        2
    );
    assert_eq!(
        reopened
            .reader()
            .unwrap()
            .read_objects(&[first.id(), second.id(), third.id()])
            .unwrap(),
        vec![
            first.canonical().to_vec(),
            second.canonical().to_vec(),
            third.canonical().to_vec()
        ]
    );
}
#[test]
fn pooled_prefix_chains_and_depth_limit_preserve_old_choices_and_bytes() {
    let (metadata, objects, storage) = setup();
    let dir = support::TempDir::new("port-pool-chain");
    let path = dir.store_path("old");
    let old = support::create_store(&path);
    let mut values = (0..40).map(value).collect::<Vec<_>>();
    let mut previous = None;
    for version in 0..12 {
        values[0] = value(1000 + version);
        let mut object = leaf(1, &values);
        if let Some(id) = previous {
            object = object.with_predecessors(AdvisoryPredecessors::explicit(id).unwrap());
        }
        let legacy = support::save_one(&old, object.clone()).unwrap();
        let result = save(&storage, vec![object.clone()]);
        assert_eq!(result.prefix_records, legacy.prefix_records);
        assert_eq!(packs(&metadata), old_packs(&path));
        previous = Some(object.id());
        let reopened = Storage::new(metadata.clone(), objects.clone()).unwrap();
        assert_eq!(
            reopened
                .reader()
                .unwrap()
                .read_objects(&[object.id()])
                .unwrap(),
            vec![object.canonical().to_vec()]
        );
    }
}
#[test]
fn zero_depth_stores_full_and_a_fifty_link_profile_reads_back() {
    for depth in [0, 50] {
        let metadata = Arc::new(MemoryMetadata::default());
        metadata.state.lock().unwrap().policy =
            layerfs_storage::StoragePolicy::frozen_default().with_metadata_depth(depth);
        let storage = Storage::new(metadata, Arc::new(MemoryObjects::default())).unwrap();
        let mut previous = None;
        for version in 0..51 {
            let mut object = leaf(1, &[value(version)]);
            if let Some(id) = previous {
                object = object.with_predecessors(AdvisoryPredecessors::explicit(id).unwrap());
            }
            let result = save(&storage, vec![object.clone()]);
            assert_eq!(result.prefix_records, u64::from(depth != 0 && version != 0));
            assert_eq!(
                storage
                    .reader()
                    .unwrap()
                    .read_objects(&[object.id()])
                    .unwrap(),
                vec![object.canonical().to_vec()]
            );
            previous = Some(object.id());
        }
    }
}
#[test]
fn ordinal_blocks_and_closure_are_bounded_across_waves() {
    let (metadata, _, storage) = setup();
    let operation = storage.begin_save().unwrap();
    let mut ids = Vec::new();
    for i in 0..1100_u64 {
        let object = leaf(i + 1, &[value(i)]);
        ids.push(object.id());
        operation.accept(object).unwrap();
        assert!(
            operation.pooled_index_bytes() <= layerfs_storage::policy::METADATA_INDEX_VALUES * 24
        );
    }
    let result = operation.finish().unwrap();
    assert_eq!(result.inserted, 1100);
    let state = metadata.state.lock().unwrap();
    assert_eq!(state.groups.values().map(|r| r.count).sum::<usize>(), 1100);
    let end = state
        .groups
        .values()
        .map(|r| u64::from(r.first_ordinal) + r.count as u64)
        .max()
        .unwrap();
    assert_eq!(state.next_ordinal, end);
    for batch in &state.registrations {
        assert!(
            batch.packs.len()
                + batch.objects.len()
                + batch.value_groups.len()
                + batch.signatures.len()
                <= layerfs_storage::policy::TRANSACTION_ROW_LIMIT as usize
        );
        for row in &batch.objects {
            assert!(state.packs.contains_key(&row.pack_id));
        }
    }
    drop(state);
    assert!(storage.diagnostics().forced_seals > 0);
    assert_eq!(
        storage
            .reader()
            .unwrap()
            .read_objects(&[ids[0], ids[1099]])
            .unwrap()
            .len(),
        2
    );
    println!("DIAGNOSTIC pooled-waves {:?}", storage.diagnostics());
}
#[test]
fn the_candidate_window_resets_at_the_existing_value_bound() {
    let (metadata, _, storage) = setup();
    let operation = storage.begin_save().unwrap();
    let mut last = None;
    for i in 0..1330_u64 {
        let object = leaf(
            i * 100 + 1,
            &(i * 100..i * 100 + 100).map(value).collect::<Vec<_>>(),
        );
        last = Some(object.id());
        operation.accept(object).unwrap();
        assert!(
            operation.pooled_index_bytes() <= layerfs_storage::policy::METADATA_INDEX_VALUES * 24
        );
    }
    operation.finish().unwrap();
    assert!(metadata.state.lock().unwrap().window > 1);
    assert_eq!(
        storage
            .reader()
            .unwrap()
            .read_objects(&[last.unwrap()])
            .unwrap()
            .len(),
        1
    );
    println!("DIAGNOSTIC pooled-window {:?}", storage.diagnostics());
}
#[test]
fn missing_or_corrupt_value_groups_fail_without_an_alternate_lookup() {
    for corrupt in [false, true] {
        let (metadata, objects, storage) = setup();
        let object = leaf(1, &[value(4)]);
        save(&storage, vec![object.clone()]);
        {
            let mut state = metadata.state.lock().unwrap();
            if corrupt {
                state.groups.values_mut().next().unwrap().digest = ObjectId::for_bytes(b"bad");
            } else {
                state.groups.clear();
            }
        }
        let reopened = Storage::new(metadata, objects).unwrap();
        assert!(reopened
            .reader()
            .unwrap()
            .read_objects(&[object.id()])
            .is_err());
        assert_eq!(reopened.diagnostics().value_groups, 1);
    }
}
#[test]
fn a_lost_metadata_acknowledgement_never_registers_a_leaf_again() {
    let (metadata, _, storage) = setup();
    metadata.state.lock().unwrap().fail_register = Some(MetadataError::Uncertain);
    let operation = storage.begin_save().unwrap();
    operation.accept(leaf(1, &[value(5)])).unwrap();
    assert!(matches!(
        operation.finish(),
        Err(StorageError::UnknownOutcome { .. })
    ));
    let state = metadata.state.lock().unwrap();
    assert!(state.objects.is_empty());
    assert!(state.groups.is_empty());
    assert_eq!(
        metadata
            .calls
            .lock()
            .unwrap()
            .iter()
            .filter(|(call, _)| *call == "register")
            .count(),
        1
    );
}
#[test]
fn real_filesystem_roots_and_every_emitted_object_match_c1_and_the_old_path() {
    for entries in [100, 1000] {
        let built = support::filesystem::build_tree(entries);
        let (metadata, objects, storage) = setup();
        let operation = storage.begin_save().unwrap();
        for object in &built.bag.finalized {
            operation.accept(object.clone()).unwrap();
        }
        operation.finish().unwrap();
        let reopened = Storage::new(metadata.clone(), objects.clone()).unwrap();
        let reader = reopened.reader().unwrap();
        for (id, (_, canonical)) in &built.bag.objects {
            assert_eq!(
                reader.read_objects(&[*id]).unwrap(),
                vec![canonical.clone()]
            );
        }
        let dir = support::TempDir::new("port-filesystem");
        let path = dir.store_path("old");
        let old = support::create_store(&path);
        support::disabled(|scope| {
            let mut save = old.begin_save(scope.child("save"))?;
            for object in &built.bag.finalized {
                save.accept(object.clone())?;
            }
            save.finish(scope.child("finish"))
        })
        .unwrap();
        assert_eq!(
            support::read_objects(&old, &[built.root]).unwrap().0,
            reader.read_objects(&[built.root]).unwrap()
        );
        assert_eq!(
            ObjectId::for_bytes(&reader.read_objects(&[built.root]).unwrap()[0]),
            built.root
        );
        let mut port_packs: Vec<_> = {
            let state = metadata.state.lock().unwrap();
            let payloads = objects.bodies.lock().unwrap();
            state
                .packs
                .values()
                .map(|pack| {
                    pack.body
                        .clone()
                        .unwrap_or_else(|| payloads[&pack.info.key].clone())
                })
                .collect()
        };
        let (legacy, payloads) = engines::snapshot(&path);
        let mut legacy_packs: Vec<_> = legacy
            .packs
            .into_iter()
            .filter_map(|pack| pack.body)
            .chain(payloads.into_iter().map(|(_, body)| body))
            .collect();
        port_packs.sort();
        legacy_packs.sort();
        assert_eq!(port_packs, legacy_packs);

        println!(
            "DIAGNOSTIC filesystem entries={} {:?}",
            entries,
            reopened.diagnostics()
        );
    }
}
