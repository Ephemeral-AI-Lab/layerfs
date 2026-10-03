//! Four acknowledgements before publication, exact sealed bytes and no retry.
#[path = "support/memory_engines.rs"]
mod engines;
mod support;
use engines::{MemoryMetadata, MemoryObjects};
use layerfs_content::{FinalizedObject, ObjectRole};
use layerfs_storage::{port::*, Storage};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc, Barrier,
};
struct FourUploads {
    inner: MemoryObjects,
    barrier: Barrier,
    calls: AtomicUsize,
    fail: bool,
}
impl ObjectStore for FourUploads {
    fn put_if_absent(&self, key: ObjectKey, body: &[u8]) -> Result<Put, ObjectError> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        self.barrier.wait();
        if self.fail {
            Err(ObjectError::Refused { status: 503 })
        } else {
            self.inner.put_if_absent(key, body)
        }
    }
    fn read(
        &self,
        key: ObjectKey,
        range: Option<ByteRange>,
        out: &mut Vec<u8>,
    ) -> Result<(), ObjectError> {
        self.inner.read(key, range, out)
    }
    fn head(&self, key: ObjectKey) -> Result<Option<u64>, ObjectError> {
        self.inner.head(key)
    }
}
#[test]
fn the_four_upload_window_is_acknowledged_before_any_registration() {
    for fail in [false, true] {
        let metadata = Arc::new(MemoryMetadata::default());
        metadata.state.lock().unwrap().policy = layerfs_storage::StoragePolicy::new(
            layerfs_storage::policy::FORMAT_PROFILE,
            1_048_576,
            0,
            0,
        )
        .validated()
        .unwrap();
        let objects = Arc::new(FourUploads {
            inner: MemoryObjects::default(),
            barrier: Barrier::new(4),
            calls: AtomicUsize::new(0),
            fail,
        });
        let storage = Storage::new(metadata.clone(), objects.clone()).unwrap();
        let save = storage.begin_parallel_save().unwrap();
        let mut ids = Vec::new();
        for n in 0..4 {
            let mut raw = support::noise(700_000);
            for b in &mut raw {
                *b ^= n * 37;
            }
            let object =
                FinalizedObject::new(ObjectRole::WholeFile, support::assembled_small_object(&raw))
                    .unwrap();
            ids.push(object.id());
            save.accept(object).unwrap();
        }
        let result = save.finish();
        assert_eq!(
            objects.calls.load(Ordering::Relaxed),
            4,
            "finish={result:?}"
        );
        if fail {
            assert!(result.is_err());
            assert!(metadata.state.lock().unwrap().objects.is_empty());
            assert!(metadata.state.lock().unwrap().registrations.is_empty());
        } else {
            assert!(result.is_ok());
            let rows = storage.reader().unwrap().read_objects(&ids).unwrap();
            assert_eq!(rows.len(), 4);
        }
    }
}
