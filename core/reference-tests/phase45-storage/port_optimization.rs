//! Work-count regression checks through ordinary public save/read APIs.
#[path = "support/memory_engines.rs"]
mod engines;
mod support;
use engines::{MemoryMetadata, MemoryObjects};
use layerfs_content::{FinalizedObject, ObjectRole};
use layerfs_storage::{policy::WAVE_CANONICAL_BYTES_LIMIT, Storage};
use std::sync::{Arc, Mutex};
#[test]
fn reservation_blocks_cover_later_waves_and_finish_without_reallocation() {
    let metadata = Arc::new(MemoryMetadata::default());
    let objects = Arc::new(MemoryObjects::default());
    let storage = Storage::new(metadata.clone(), objects.clone()).unwrap();
    let save = storage.begin_save().unwrap();
    let mut ids = Vec::new();
    for i in 0..1600_u32 {
        let mut raw = vec![3; 600];
        raw[..4].copy_from_slice(&i.to_be_bytes());
        let object =
            FinalizedObject::new(ObjectRole::WholeFile, support::assembled_small_object(&raw))
                .unwrap();
        ids.push((object.id(), object.canonical().to_vec()));
        save.accept(object).unwrap();
        assert!(save.pending_canonical_bytes() <= WAVE_CANONICAL_BYTES_LIMIT);
    }
    let outcome = save.finish().unwrap();
    assert_eq!(outcome.inserted, 1600);
    let calls = metadata.calls.lock().unwrap();
    let reserves = calls.iter().filter(|(name, _)| *name == "reserve").count();
    assert_eq!(
        reserves, 1,
        "unused IDs must cover later waves and final seals"
    );
    drop(calls);
    let reopened = Storage::new(metadata, objects).unwrap();
    for (id, bytes) in ids.iter().step_by(64) {
        assert_eq!(
            reopened.reader().unwrap().read_objects(&[*id]).unwrap(),
            vec![bytes.clone()]
        );
    }
}

#[test]
fn mixed_file_lanes_share_budget_and_reopen_exact_bytes_with_fewer_payload_packs() {
    let metadata = Arc::new(MemoryMetadata::default());
    let objects = Arc::new(MemoryObjects::default());
    let storage = Storage::new(metadata.clone(), objects.clone()).unwrap();
    let dir = support::TempDir::new("mixed-lanes");
    let path = dir.store_path("old");
    let old = support::create_store(&path);
    let operation = storage.begin_save().unwrap();
    let mut expected = Vec::new();
    let mut legacy = Vec::new();
    for i in 0..20_u32 {
        let mut raw = support::noise(if i % 2 == 0 { 96000 } else { 200000 });
        raw[..4].copy_from_slice(&i.to_be_bytes());
        let (bag, root, _) = support::construct_file(&raw);
        for object in bag.finalized() {
            operation.accept(object.clone()).unwrap();
            legacy.push(object);
        }
        assert!(operation.pending_canonical_bytes() <= WAVE_CANONICAL_BYTES_LIMIT);
        expected.push((root, raw));
    }
    operation.finish().unwrap();
    support::disabled(|scope| {
        let mut save = old.begin_save(scope.child("save"))?;
        for object in legacy {
            save.accept(object)?;
        }
        save.finish(scope.child("finish"))
    })
    .unwrap();
    let (_, payloads) = engines::snapshot(&path);
    let actual = objects.bodies.lock().unwrap().len();
    assert!(
        actual < payloads.len(),
        "mixed lane flushes: current {actual}, reference {}",
        payloads.len()
    );
    let reopened = Storage::new(metadata, objects).unwrap();
    for (root, raw) in expected {
        let mut bytes = Vec::new();
        support::disabled(|scope| {
            layerfs_content::read_all(
                &reopened.reader().unwrap(),
                root,
                &mut bytes,
                scope.child("read"),
            )
        })
        .unwrap();
        assert_eq!(bytes, raw);
    }
}

struct GatedObjects {
    inner: MemoryObjects,
    next: std::sync::atomic::AtomicUsize,
    active: std::sync::atomic::AtomicUsize,
    peak: std::sync::atomic::AtomicUsize,
    starts: std::sync::mpsc::Sender<usize>,
    release: std::sync::Mutex<std::sync::mpsc::Receiver<()>>,
}
impl layerfs_storage::port::ObjectStore for GatedObjects {
    fn put_if_absent(
        &self,
        key: layerfs_storage::port::ObjectKey,
        body: &[u8],
    ) -> Result<layerfs_storage::port::Put, layerfs_storage::port::ObjectError> {
        use std::sync::atomic::Ordering::SeqCst;
        let n = self.next.fetch_add(1, SeqCst);
        let active = self.active.fetch_add(1, SeqCst) + 1;
        self.peak.fetch_max(active, SeqCst);
        self.starts.send(n).unwrap();
        if n == 0 {
            self.release
                .lock()
                .unwrap()
                .recv_timeout(std::time::Duration::from_secs(10))
                .unwrap();
        }
        let result = self.inner.put_if_absent(key, body);
        self.active.fetch_sub(1, SeqCst);
        result
    }
    fn read(
        &self,
        key: layerfs_storage::port::ObjectKey,
        range: Option<layerfs_storage::port::ByteRange>,
        out: &mut Vec<u8>,
    ) -> Result<(), layerfs_storage::port::ObjectError> {
        self.inner.read(key, range, out)
    }
    fn head(
        &self,
        key: layerfs_storage::port::ObjectKey,
    ) -> Result<Option<u64>, layerfs_storage::port::ObjectError> {
        self.inner.head(key)
    }
}
#[test]
fn rolling_upload_admits_fifth_pack_before_slow_first_finishes_with_at_most_four_calls() {
    use std::sync::atomic::Ordering::SeqCst;
    let (starts, observed) = std::sync::mpsc::channel();
    let (release, gate) = std::sync::mpsc::channel();
    let objects = Arc::new(GatedObjects {
        inner: MemoryObjects::default(),
        next: 0.into(),
        active: 0.into(),
        peak: 0.into(),
        starts,
        release: Mutex::new(gate),
    });
    let worker_objects = objects.clone();
    let worker = std::thread::spawn(move || {
        let metadata = Arc::new(MemoryMetadata::default());
        let storage = Storage::new(metadata.clone(), worker_objects).unwrap();
        let save = storage.begin_parallel_save().unwrap();
        for i in 0..20_u8 {
            let raw = support::noise(96_000)
                .into_iter()
                .map(|b| b.wrapping_add(i.wrapping_mul(17)))
                .collect::<Vec<_>>();
            save.accept(
                FinalizedObject::new(ObjectRole::WholeFile, support::assembled_small_object(&raw))
                    .unwrap(),
            )
            .unwrap();
        }
        save.finish().unwrap();
        let count = metadata.state.lock().unwrap().objects.len();
        count
    });
    let mut fifth = false;
    for _ in 0..5 {
        match observed.recv_timeout(std::time::Duration::from_secs(2)) {
            Ok(n) if n >= 4 => {
                fifth = true;
                break;
            }
            Ok(_) => {}
            Err(_) => break,
        }
    }
    release.send(()).unwrap();
    let count = worker.join().unwrap();
    assert!(
        fifth,
        "a completed slot must admit pack five while pack one is still gated"
    );
    assert!(objects.peak.load(SeqCst) <= 4);
    assert_eq!(objects.active.load(SeqCst), 0);
    assert_eq!(count, 20);
}
