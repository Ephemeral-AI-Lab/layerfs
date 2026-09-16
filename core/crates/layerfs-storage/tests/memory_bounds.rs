//! Declared capacities, bounded ownership and the no-spill singleton path.
//!
//! Evidence here is allocation accounting, not resident-set size: the assertions
//! cover declared limits, live ownership and the absence of payload files. Heap
//! and OS RSS are deliberately not claimed, because this target cannot attribute
//! them to the product without a profiler.

mod support;

use std::path::Path;

use layerfs_content::{object::codec::encode_bytes_object, FinalizedObject, ObjectRole};
use layerfs_storage::{StorageError, Store};
use support::{
    construct_file, create_store, disabled, noise, open_store, read_objects, save_all, TempDir,
};

fn whole_file_object(raw: &[u8]) -> FinalizedObject {
    let mut value = Vec::new();
    value.extend_from_slice(b"LFS5SML\0");
    value.extend_from_slice(&1u16.to_be_bytes());
    value.extend_from_slice(raw);
    FinalizedObject::new(
        ObjectRole::WholeFile,
        encode_bytes_object(&value).expect("canonical envelope"),
    )
    .expect("canonical object")
}

fn chunk_object(raw: &[u8]) -> FinalizedObject {
    let mut value = Vec::new();
    value.extend_from_slice(b"LFS4CHK\0");
    value.extend_from_slice(raw);
    FinalizedObject::new(
        ObjectRole::Chunk,
        encode_bytes_object(&value).expect("canonical envelope"),
    )
    .expect("canonical object")
}

fn files_in(path: &Path) -> Vec<String> {
    std::fs::read_dir(path)
        .expect("store directory")
        .map(|entry| {
            entry
                .expect("entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect()
}

#[test]
fn an_oversized_whole_file_record_is_rejected_against_its_declared_limit() {
    let dir = TempDir::new("oversize_whole");
    let path = dir.store_path("oversize_whole");
    let store = create_store(&path);
    let object = whole_file_object(&vec![0x11; 131_072]);
    let error = disabled(|scope| {
        let mut operation = store.begin_save(scope.child("storage.begin"))?;
        operation.accept(object, scope.child("storage.accept"))?;
        operation.finish(scope.child("storage.finish"))
    })
    .unwrap_err();
    match error {
        StorageError::CapacityExceeded {
            what,
            limit,
            actual,
        } => {
            assert_eq!(what, "encoding.whole_file_raw");
            assert_eq!(limit, 131_071);
            assert_eq!(actual, 131_072);
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn an_oversized_chunk_record_is_rejected_against_its_declared_limit() {
    let dir = TempDir::new("oversize_chunk");
    let path = dir.store_path("oversize_chunk");
    let store = create_store(&path);
    let object = chunk_object(&vec![0x22; 32_769]);
    let error = disabled(|scope| {
        let mut operation = store.begin_save(scope.child("storage.begin"))?;
        operation.accept(object, scope.child("storage.accept"))?;
        operation.finish(scope.child("storage.finish"))
    })
    .unwrap_err();
    assert!(
        matches!(
            error,
            StorageError::Content(layerfs_content::ContentError::ObjectLimitExceeded {
                limit: 32_768,
                actual: 32_769,
            })
        ),
        "got {error}"
    );
}

#[test]
fn pending_ownership_stays_inside_the_declared_batch_bounds() {
    let dir = TempDir::new("batch");
    let path = dir.store_path("batch");
    let store = create_store(&path);
    let limits = store.capacities();
    let mut operation = disabled(|scope| store.begin_save(scope.child("storage.begin"))).unwrap();
    let mut peak_objects = 0usize;
    let mut peak_bytes = 0u64;
    for index in 0..200u16 {
        let mut payload = noise(6_000);
        payload[0] = index as u8;
        payload[1] = (index >> 8) as u8;
        let object = whole_file_object(&payload);
        if disabled(|scope| operation.accept(object, scope.child("storage.accept"))).is_err() {
            break;
        }
        let (objects, bytes) = operation.pending();
        peak_objects = peak_objects.max(objects);
        peak_bytes = peak_bytes.max(bytes);
        assert!(
            objects <= limits.batch_objects,
            "pending objects {objects} exceed {}",
            limits.batch_objects
        );
        assert!(bytes <= limits.batch_bytes, "pending bytes {bytes}");
        let retained = operation.retained_tail_bytes().unwrap();
        assert!(
            retained <= 3 * limits.pack_limit,
            "retained tail {retained} exceeds one pack per framing lane"
        );
    }
    let outcome = disabled(|scope| operation.finish(scope.child("storage.finish"))).unwrap();
    assert!(outcome.inserted > 0);
    assert!(peak_objects <= limits.batch_objects);
    assert!(peak_bytes <= limits.batch_bytes);
}

#[test]
fn the_supported_incompressible_singletons_are_stored_and_read_back() {
    let dir = TempDir::new("singleton");
    let path = dir.store_path("singleton");
    let store = create_store(&path);

    let whole_raw = noise(131_071);
    let whole = whole_file_object(&whole_raw);
    let whole_id = whole.id();
    let chunk_raw = noise(32_768);
    let chunk = chunk_object(&chunk_raw);
    let chunk_id = chunk.id();

    let outcome = disabled(|scope| {
        let mut operation = store.begin_save(scope.child("storage.begin"))?;
        operation.accept(whole, scope.child("storage.accept"))?;
        operation.accept(chunk, scope.child("storage.accept"))?;
        operation.finish(scope.child("storage.finish"))
    })
    .expect("supported singletons must store");
    assert_eq!(outcome.inserted, 2);

    let ids = [whole_id, chunk_id];
    let (values, _) = read_objects(&store, &ids).unwrap();
    assert_eq!(ObjectRole::WholeFile.code(), 1);
    assert_eq!(values.len(), 2);
    assert_eq!(
        layerfs_content::whole_file_payload(&values[0])
            .unwrap()
            .unwrap(),
        whole_raw.as_slice()
    );
    let value = layerfs_content::object::codec::decode_bytes_object(&values[1]).unwrap();
    assert_eq!(
        layerfs_content::file::mapping::decode_chunk_payload(value).unwrap(),
        chunk_raw.as_slice()
    );
    assert_eq!(values[0].len(), 131_094);
    assert_eq!(values[1].len(), 32_789);
}

#[test]
fn a_complete_file_singleton_never_creates_a_payload_file() {
    let dir = TempDir::new("nospill");
    let path = dir.store_path("nospill");
    let store = create_store(&path);
    let bytes = noise(131_071);
    let (collected, root, _) = construct_file(&bytes);
    assert_eq!(collected.objects().len(), 1);
    save_all(&store, &collected).expect("save succeeds");
    drop(store);

    let mut entries = files_in(dir.path());
    entries.sort();
    assert_eq!(
        entries,
        vec!["nospill.sqlite".to_string()],
        "no temporary singleton pack file and no spool remain"
    );

    let reopened = open_store(&path);
    let (values, _) = read_objects(&reopened, &[root]).unwrap();
    assert_eq!(values[0], collected.objects()[0].2);
}

#[test]
fn a_large_input_is_reachable_only_through_its_own_singleton_path() {
    let dir = TempDir::new("large");
    let path = dir.store_path("large");
    let store = create_store(&path);
    for size in [131_071usize, 98_304, 65_536] {
        let payload = noise(size);
        let object = whole_file_object(&payload);
        let id = object.id();
        disabled(|scope| {
            let mut operation = store.begin_save(scope.child("storage.begin"))?;
            operation.accept(object, scope.child("storage.accept"))?;
            operation.finish(scope.child("storage.finish"))
        })
        .unwrap();
        let (values, _) = read_objects(&store, &[id]).unwrap();
        assert_eq!(values[0], collected_bytes(&payload));
    }
}

fn collected_bytes(raw: &[u8]) -> Vec<u8> {
    let mut value = Vec::new();
    value.extend_from_slice(b"LFS5SML\0");
    value.extend_from_slice(&1u16.to_be_bytes());
    value.extend_from_slice(raw);
    encode_bytes_object(&value).unwrap()
}

#[test]
fn a_store_created_with_an_unsupported_policy_is_rejected_before_work() {
    let dir = TempDir::new("policy");
    let path = dir.store_path("policy");
    let unsupported = layerfs_storage::StoragePolicy::new(1, 1_048_577, 8, 4);
    let error =
        disabled(|scope| Store::create(&path, unsupported, scope.child("store"))).unwrap_err();
    assert!(
        matches!(error, StorageError::UnsupportedPolicy { field } if field == "small_file_threshold_bytes"),
        "got {error}"
    );
    assert!(
        !path.exists(),
        "nothing was created for an unsupported policy"
    );
}
