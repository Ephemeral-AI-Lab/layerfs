//! Count/cause fixture for locator retention at the existing 4096-entry bound.
use crate::chains;
use crate::metadata;
#[path = "../support/mod.rs"]
mod support;
use layerfs_content::{
    inode_leaf::{
        encode_inode_value, InodeKind, InodeLeaf, InodeLeafRow, InodeMetadata, InodeValue,
    },
    FinalizedConsumer, FinalizedObject, ObjectId, ObjectRole,
};
use layerfs_storage::{policy, Storage};
use std::sync::Arc;

#[test]
fn pressure_admission_preserves_an_unrequested_positive_locator() {
    let metadata = Arc::new(metadata::MemoryMetadata::default());
    let writer = Storage::new(metadata.clone()).unwrap();
    let mut objects: Vec<_> = (0..policy::READ_OBJECT_LIMIT as u64 + 1)
        .map(|n| {
            FinalizedObject::new(
                ObjectRole::FileState,
                layerfs_content::object::codec::encode_bytes_object(&n.to_be_bytes()).unwrap(),
            )
            .unwrap()
        })
        .collect();
    let save = writer.begin_save().unwrap();
    for object in &objects {
        save.accept(object.clone()).unwrap();
    }
    save.finish().unwrap();
    objects.sort_by_key(FinalizedObject::id);
    let reading = Storage::new(metadata.clone()).unwrap();
    let reader = reading.reader().unwrap();
    let ids: Vec<_> = objects[..policy::READ_OBJECT_LIMIT]
        .iter()
        .map(FinalizedObject::id)
        .collect();
    let actual = reader.read_objects(&ids).unwrap();
    assert_eq!(
        actual,
        objects[..policy::READ_OBJECT_LIMIT]
            .iter()
            .map(|o| o.canonical().to_vec())
            .collect::<Vec<_>>()
    );
    metadata.calls.lock().unwrap().clear();
    let miss = &objects[policy::READ_OBJECT_LIMIT];
    assert_eq!(
        reader.read_objects(&[miss.id()]).unwrap(),
        vec![miss.canonical().to_vec()]
    );
    let retained = &objects[policy::READ_OBJECT_LIMIT - 1];
    assert_eq!(
        reader.read_objects(&[retained.id()]).unwrap(),
        vec![retained.canonical().to_vec()]
    );
    let requests: Vec<_> = metadata
        .calls
        .lock()
        .unwrap()
        .iter()
        .filter(|(name, _)| *name == "locate")
        .copied()
        .collect();
    eprintln!("DIAGNOSTIC locator-pressure requests={requests:?}");
    assert_eq!(requests, vec![("locate", 1)]);
    assert_eq!(reading.diagnostics().locator_evictions, 1);
}

#[test]
fn second_chance_breaks_repeated_low_key_victim_churn() {
    let metadata = Arc::new(metadata::MemoryMetadata::default());
    let writer = Storage::new(metadata.clone()).unwrap();
    let mut objects: Vec<_> = (0..policy::READ_OBJECT_LIMIT as u64 + 1)
        .map(|n| {
            FinalizedObject::new(
                ObjectRole::FileState,
                layerfs_content::object::codec::encode_bytes_object(&n.to_be_bytes()).unwrap(),
            )
            .unwrap()
        })
        .collect();
    let save = writer.begin_save().unwrap();
    for object in &objects {
        save.accept(object.clone()).unwrap();
    }
    save.finish().unwrap();
    objects.sort_by_key(FinalizedObject::id);
    let reading = Storage::new(metadata.clone()).unwrap();
    let reader = reading.reader().unwrap();
    let ids: Vec<_> = objects[..policy::READ_OBJECT_LIMIT]
        .iter()
        .map(FinalizedObject::id)
        .collect();
    let actual = reader.read_objects(&ids).unwrap();
    assert_eq!(
        actual,
        objects[..policy::READ_OBJECT_LIMIT]
            .iter()
            .map(|o| o.canonical().to_vec())
            .collect::<Vec<_>>()
    );
    metadata.calls.lock().unwrap().clear();
    let miss = &objects[policy::READ_OBJECT_LIMIT];
    assert_eq!(
        reader.read_objects(&[miss.id()]).unwrap(),
        vec![miss.canonical().to_vec()]
    );
    let retained = &objects[0];
    assert_eq!(
        reader.read_objects(&[retained.id()]).unwrap(),
        vec![retained.canonical().to_vec()]
    );
    for _ in 0..16 {
        for object in [&objects[1], &objects[0]] {
            assert_eq!(
                reader.read_objects(&[object.id()]).unwrap(),
                vec![object.canonical().to_vec()]
            );
        }
    }
    let requests: Vec<_> = metadata
        .calls
        .lock()
        .unwrap()
        .iter()
        .filter(|(name, _)| *name == "locate")
        .copied()
        .collect();
    eprintln!("DIAGNOSTIC locator-pressure requests={requests:?}");
    assert_eq!(requests, vec![("locate", 1); 3]);
    assert_eq!(reading.diagnostics().locator_evictions, 3);
    assert!(
        reading.diagnostics().locator_eviction_probes <= 2 * policy::READ_OBJECT_LIMIT as u64 + 3
    );
    assert!(
        reading.diagnostics().locator_bookkeeping_live_peak_bytes
            <= policy::READ_OBJECT_LIMIT as u64 * 8 + 64
    );
}

#[test]
fn a_network_batched_reader_prefers_an_authoritative_inode_locator_at_the_same_bound() {
    let memory = Arc::new(metadata::MemoryMetadata::default());
    let writer = Storage::new(memory.clone()).unwrap();
    let value = encode_inode_value(InodeValue {
        kind: InodeKind::RegularFile,
        namespace_ref_count: 1,
        content_root: ObjectId::for_bytes(b"content"),
        metadata_root: InodeMetadata::Root(ObjectId::for_bytes(b"metadata")),
    });
    let inode = (1..=32)
        .map(|serial| {
            let leaf = InodeLeaf {
                subtree_bytes: layerfs_content::inode_leaf::LEAF_ROW_BYTES as u64,
                rows: vec![InodeLeafRow { serial, value }],
            };
            FinalizedObject::new(ObjectRole::InodeLeaf, leaf.encode().unwrap()).unwrap()
        })
        .min_by_key(FinalizedObject::id)
        .unwrap();
    // Make the inode the first unprotected candidate in object-id order.
    let payloads: Vec<_> = (0..policy::READ_OBJECT_LIMIT as u64 * 2)
        .map(|serial| {
            FinalizedObject::new(
                ObjectRole::FileState,
                layerfs_content::object::codec::encode_bytes_object(&serial.to_be_bytes()).unwrap(),
            )
            .unwrap()
        })
        .filter(|object| object.id() > inode.id())
        .take(policy::READ_OBJECT_LIMIT)
        .collect();
    assert_eq!(payloads.len(), policy::READ_OBJECT_LIMIT);
    let save = writer.begin_save().unwrap();
    for object in std::iter::once(&inode).chain(&payloads) {
        save.accept(object.clone()).unwrap();
    }
    save.finish().unwrap();

    let fill: Vec<_> = std::iter::once(&inode)
        .chain(&payloads[..policy::READ_OBJECT_LIMIT - 1])
        .map(FinalizedObject::id)
        .collect();
    let miss = payloads.last().unwrap();
    let mut peaks = Vec::new();
    for (batched, prefer_inodes) in [(false, true), (true, false), (true, true)] {
        let mut provider = chains::ChainProvider::new(memory.clone());
        provider.chains = false;
        provider.batched = batched;
        let provider = Arc::new(provider);
        let reading = Storage::new(provider.clone()).unwrap();
        let reading = if prefer_inodes {
            reading.retaining_inode_locators()
        } else {
            reading
        };
        let reader = reading.reader().unwrap();
        reader.read_objects(&fill).unwrap();
        provider.asked.lock().unwrap().clear();

        assert_eq!(
            reader.read_objects(&[miss.id()]).unwrap(),
            vec![miss.canonical().to_vec()]
        );
        assert_eq!(reading.diagnostics().locator_evictions, 1);
        assert_eq!(
            reader.read_objects(&[inode.id()]).unwrap(),
            vec![inode.canonical().to_vec()]
        );

        let mut expected = vec![vec![miss.id()]];
        if !(batched && prefer_inodes) {
            expected.push(vec![inode.id()]);
        }
        assert_eq!(*provider.asked.lock().unwrap(), expected);
        peaks.push(reading.diagnostics().locator_bookkeeping_live_peak_bytes);
    }
    assert_eq!(peaks, [peaks[0]; 3]);
}

#[test]
fn a_captured_full_ring_winner_keeps_one_extra_clock_pass() {
    let object = |raw: &[u8]| {
        FinalizedObject::new(ObjectRole::WholeFile, support::assembled_small_object(raw)).unwrap()
    };
    let (raw, winner) = (1..=32)
        .map(|n| {
            let mut raw = support::noise(8_000);
            raw[40] ^= n;
            let winner = object(&raw);
            (raw, winner)
        })
        .min_by_key(|(_, winner)| winner.id())
        .unwrap();
    let mut changed = raw;
    changed[80] ^= 0x51;
    let related = object(&changed);

    let pressure: Vec<_> = (0..policy::READ_OBJECT_LIMIT as u64 * 2)
        .map(|serial| {
            FinalizedObject::new(
                ObjectRole::FileState,
                layerfs_content::object::codec::encode_bytes_object(&serial.to_be_bytes()).unwrap(),
            )
            .unwrap()
        })
        .filter(|row| row.id() > winner.id())
        .take(policy::READ_OBJECT_LIMIT)
        .collect();
    assert_eq!(pressure.len(), policy::READ_OBJECT_LIMIT);
    let ids: Vec<_> = pressure[..policy::READ_OBJECT_LIMIT - 1]
        .iter()
        .map(FinalizedObject::id)
        .collect();

    for captured in [false, true] {
        let memory = Arc::new(metadata::MemoryMetadata::default());
        let writer = Storage::new(memory.clone()).unwrap();
        let seed = writer.begin_save().unwrap();
        for row in &pressure {
            seed.accept(row.clone()).unwrap();
        }
        seed.finish().unwrap();

        let mut provider = chains::ChainProvider::new(memory.clone());
        provider.chains = false;
        provider.batched = true;
        let provider = Arc::new(provider);
        let storage = Storage::new(provider.clone())
            .unwrap()
            .retaining_inode_locators();
        let save = storage.begin_save().unwrap();
        let mut sink = if captured {
            save.captured_sink()
        } else {
            save.sink()
        };
        sink.accept(winner.clone()).unwrap();
        sink.accept(related.clone()).unwrap();
        // Forces one wave. The related file waits for its FULL base, so the
        // acknowledged publication carries the winner and its signature.
        for _ in 0..policy::BATCH_OBJECT_LIMIT - 1 {
            sink.accept(pressure[0].clone()).unwrap();
        }
        assert!(memory
            .state
            .lock()
            .unwrap()
            .signatures
            .values()
            .any(|signature| signature.object_id == winner.id()));

        let reader = storage.reader().unwrap();
        reader.read_objects(&ids).unwrap();
        provider.asked.lock().unwrap().clear();
        let miss = pressure.last().unwrap().id();
        reader.read_objects(&[miss]).unwrap();
        assert_eq!(
            reader.read_objects(&[winner.id()]).unwrap(),
            vec![winner.canonical().to_vec()]
        );

        let mut expected = vec![vec![miss]];
        if !captured {
            expected.push(vec![winner.id()]);
        }
        assert_eq!(*provider.asked.lock().unwrap(), expected);
    }
}
