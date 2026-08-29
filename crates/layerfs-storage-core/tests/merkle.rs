use layerfs_core::content::rope::{visit_extents, FileStateRoot};
use layerfs_core::logical::{self, LogicalCounters};
use layerfs_core::object::access::ObjectRead;
use layerfs_core::{CanonicalPath, ObjectId};
use layerfs_storage_core::{
    apply_changes, empty_root, Change, CoreReader, ObjectSource, Result, StorageError,
};
use std::collections::BTreeMap;

#[derive(Default)]
struct Memory(BTreeMap<ObjectId, Vec<u8>>);

impl ObjectSource for Memory {
    fn read_object(&self, id: ObjectId) -> Result<Vec<u8>> {
        self.0
            .get(&id)
            .cloned()
            .ok_or(StorageError::MissingBaseData)
    }
}

fn admit(memory: &mut Memory, built: layerfs_storage_core::BuiltRoot) -> ObjectId {
    built
        .objects
        .visit_batches(&mut |objects, _| {
            for object in objects {
                memory.0.insert(object.id, object.bytes.clone());
            }
            Ok(())
        })
        .unwrap();
    built.root_id
}

fn extents(memory: &Memory, root: ObjectId) -> Vec<ObjectId> {
    let reader = CoreReader(memory);
    let resolved = logical::resolve(
        &reader,
        root,
        &CanonicalPath::new("large").unwrap(),
        &mut LogicalCounters::default(),
    )
    .unwrap();
    let mut ids = Vec::new();
    visit_extents(
        &reader,
        FileStateRoot(resolved.record.content_root),
        |extents| {
            ids.extend(extents.iter().map(|extent| extent.payload_object_id));
            Ok(())
        },
    )
    .unwrap();
    ids
}

#[test]
fn splice_scans_only_replacement_and_reuses_suffix() {
    let mut memory = Memory::default();
    let empty = empty_root([1; 32]).unwrap();
    let empty = admit(&mut memory, empty);
    let built = apply_changes(
        &memory,
        empty,
        &[Change::Write {
            path: "large".into(),
            bytes: vec![7; 80_000],
            mode: 0o644,
        }],
        [2; 32],
    )
    .unwrap();
    let base = admit(&mut memory, built);
    let before = extents(&memory, base);
    let edited = apply_changes(
        &memory,
        base,
        &[Change::Splice {
            path: "large".into(),
            start: 1,
            delete_len: 1,
            replacement: vec![9],
        }],
        [3; 32],
    )
    .unwrap();
    assert_eq!(edited.counters.cdc_bytes_scanned, 1);
    let edited_root = admit(&mut memory, edited);
    let after = extents(&memory, edited_root);
    assert_eq!(before.last(), after.last());
}

#[test]
fn logical_roots_are_canonical_and_deterministic() {
    let mut memory = Memory::default();
    let empty = empty_root([7; 32]).unwrap();
    let empty_root_id = empty.root_id;
    admit(&mut memory, empty);
    let changes = [
        Change::Mkdir {
            path: "a".into(),
            mode: 0o755,
        },
        Change::Write {
            path: "a/file".into(),
            bytes: b"hello".to_vec(),
            mode: 0o644,
        },
    ];
    let first = apply_changes(&memory, empty_root_id, &changes, [8; 32]).unwrap();
    let second = apply_changes(&memory, empty_root_id, &changes, [8; 32]).unwrap();
    assert_eq!(first.root_id, second.root_id);
    logical::namespace(&CoreReader(&memory), empty_root_id).unwrap();
}

#[test]
fn candidate_keeps_only_objects_reachable_from_the_final_root() {
    let mut memory = Memory::default();
    let empty = empty_root([21; 32]).unwrap();
    let empty = admit(&mut memory, empty);
    let temporary = layerfs_core::encode_bytes_object(b"temporary-only").unwrap();
    let temporary_id = ObjectId::for_bytes(&temporary);
    let built = apply_changes(
        &memory,
        empty,
        &[
            Change::Write {
                path: "temporary".into(),
                bytes: b"temporary-only".to_vec(),
                mode: 0o600,
            },
            Change::Remove {
                path: "temporary".into(),
            },
            Change::Write {
                path: "kept".into(),
                bytes: b"kept".to_vec(),
                mode: 0o600,
            },
        ],
        [22; 32],
    )
    .unwrap();
    let mut ids = Vec::new();
    built
        .objects
        .visit_batches(&mut |objects, _| {
            ids.extend(objects.iter().map(|object| object.id));
            Ok(())
        })
        .unwrap();
    assert!(!ids.contains(&temporary_id));
    assert!(!ids.is_empty());
}

#[test]
fn core_reader_fetches_64_payloads_in_one_source_batch() {
    struct BatchSource {
        objects: BTreeMap<ObjectId, Vec<u8>>,
        batches: std::cell::Cell<usize>,
    }

    impl ObjectSource for BatchSource {
        fn read_object(&self, _id: ObjectId) -> Result<Vec<u8>> {
            panic!("batch override must avoid per-object reads")
        }

        fn read_objects(
            &self,
            ids: &[ObjectId],
        ) -> Result<Vec<layerfs_storage_core::CanonicalObject>> {
            self.batches.set(self.batches.get() + 1);
            ids.iter()
                .map(|id| {
                    Ok(layerfs_storage_core::CanonicalObject {
                        id: *id,
                        bytes: self.objects[id].clone(),
                    })
                })
                .collect()
        }
    }

    let mut objects = BTreeMap::new();
    for index in 0..64_u8 {
        let bytes = layerfs_core::encode_bytes_object(&[index]).unwrap();
        objects.insert(ObjectId::for_bytes(&bytes), bytes);
    }
    let ids = objects.keys().copied().collect::<Vec<_>>();
    let source = BatchSource {
        objects,
        batches: std::cell::Cell::new(0),
    };
    let mut visited = 0;
    CoreReader(&source)
        .get_authenticated_batch(&ids, |_, payload| {
            assert_eq!(payload.len(), 1);
            visited += 1;
            Ok(())
        })
        .unwrap();
    assert_eq!(visited, 64);
    assert_eq!(source.batches.get(), 1);
}
