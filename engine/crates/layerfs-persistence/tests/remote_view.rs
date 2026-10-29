//! #510: a Store read in place through the `layerfs-remote` VFS, from a
//! source that serves byte ranges of a pushed (VACUUM INTO) copy: the same
//! bytes as the local file, writes refused, a changed file reported
//! `Changed`, and the block cache's merging and bound.
#![cfg(any(target_os = "macos", target_os = "linux"))]
mod support;

use layerfs_content::{construct_bytes, read_range, ObjectId};
use layerfs_history::HistoryCatalogConfig;
use layerfs_persistence::remote::cache::{plan, Run};
use layerfs_persistence::remote::{
    BlockCache, RemoteFailure, RemoteFailureKind, RemoteFile, RemoteSource, BLOCK, MAX_REQUEST,
};
use layerfs_persistence::{
    Handles, PersistenceConfig, SqliteAcquisitionSchema, SqlitePackLayout, SqlitePersistenceProfile,
};
use layerfs_storage::{Storage, StoragePolicy};
use layerfs_telemetry::timer::Timing;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use support::Temp;

const BINDING: &[u8] = b"remote-view-binding";
const CURSOR: [u8; 32] = [31; 32];

/// The bytes of one file version, served by range, counting requests.
struct Bytes {
    data: Vec<u8>,
    requests: AtomicU64,
    spans: Mutex<Vec<(u64, u64)>>,
    changed: AtomicBool,
}

impl Bytes {
    fn new(data: Vec<u8>) -> Arc<Self> {
        Arc::new(Self {
            data,
            requests: AtomicU64::new(0),
            spans: Mutex::new(Vec::new()),
            changed: AtomicBool::new(false),
        })
    }
}

impl RemoteSource for Bytes {
    fn bytes(&self) -> u64 {
        self.data.len() as u64
    }
    fn fetch(&self, offset: u64, out: &mut [u8]) -> Result<(), RemoteFailure> {
        self.requests.fetch_add(1, Ordering::Relaxed);
        self.spans.lock().unwrap().push((offset, out.len() as u64));
        if self.changed.load(Ordering::Relaxed) {
            return Err(RemoteFailure {
                kind: RemoteFailureKind::Changed,
                message: "412".to_owned(),
            });
        }
        let start = offset as usize;
        out.copy_from_slice(&self.data[start..start + out.len()]);
        Ok(())
    }
}

fn config(path: std::path::PathBuf) -> PersistenceConfig {
    PersistenceConfig::sqlite(path)
        .with_sqlite_profile(SqlitePersistenceProfile::Disposable)
        .with_sqlite_pack_layout(SqlitePackLayout::GroupRowsIndexed)
        .with_sqlite_acquisition(SqliteAcquisitionSchema::Tables)
}

fn save(storage: &Storage, bytes: &[u8]) -> ObjectId {
    let save = storage.begin_save().unwrap();
    let root = Timing::disabled("construct", |scope| {
        construct_bytes(
            storage.policy().construction(),
            &storage.policy().construction().capacities(),
            bytes,
            &mut save.sink(),
            scope.child("content"),
        )
    })
    .0
    .unwrap()
    .root;
    save.finish().unwrap();
    root
}

fn read(storage: &Storage, id: ObjectId, len: u64) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    Timing::disabled("read", |scope| {
        read_range(
            &storage.reader().map_err(|e| format!("{e:?}"))?,
            id,
            0..len,
            &mut bytes,
            scope.child("read"),
        )
        .map_err(|e| format!("{e:?}"))
    })
    .0?;
    Ok(bytes)
}

/// Bytes that do not compress (xorshift).
fn pattern(len: usize, seed: u32) -> Vec<u8> {
    let mut state = 0x9e37_79b9_7f4a_7c15_u64 ^ u64::from(seed);
    (0..len)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            (state >> 24) as u8
        })
        .collect()
}

#[test]
fn a_pushed_store_reads_in_place_as_the_local_file() {
    let temp = Temp::new("remote-view");
    let handles = Handles::create(
        config(temp.join("store")),
        StoragePolicy::frozen_default(),
        &HistoryCatalogConfig {
            binding_key: BINDING.to_vec(),
            cursor_key: CURSOR,
            incarnation: 3,
        },
    )
    .unwrap();
    let storage = Storage::new(handles.storage.clone()).unwrap();
    let small = pattern(900, 1);
    let large = pattern(3_000_000, 2);
    let (small_id, large_id) = (save(&storage, &small), save(&storage, &large));
    let pushed = handles.snapshot(&temp.join("pushed")).unwrap();
    let source = Bytes::new(std::fs::read(&pushed.path).unwrap());

    let file = RemoteFile::new(source.clone(), 8 * 1024 * 1024);
    let view = Handles::open_remote(&file, BINDING, CURSOR).unwrap();
    let viewed = Storage::new(view.storage.clone()).unwrap();
    assert_eq!(read(&viewed, small_id, small.len() as u64).unwrap(), small);
    assert_eq!(read(&viewed, large_id, large.len() as u64).unwrap(), large);
    assert_eq!(view.profile().pack_layout, handles.profile().pack_layout);
    // every request is one ranged read of at most 1 MiB, block aligned
    for (offset, length) in source.spans.lock().unwrap().iter() {
        assert_eq!(offset % BLOCK, 0);
        assert!(*length <= MAX_REQUEST);
    }
    let stats = file.stats();
    assert_eq!(stats.fetches, source.requests.load(Ordering::Relaxed));
    assert!(stats.resident <= stats.capacity);
    assert!(stats.hits > 0);

    // Read again: everything is cached, no request is made.
    let before = source.requests.load(Ordering::Relaxed);
    assert_eq!(read(&viewed, small_id, small.len() as u64).unwrap(), small);
    assert_eq!(source.requests.load(Ordering::Relaxed), before);

    // A write is refused; nothing reaches the source as a write.
    assert!(viewed
        .begin_save()
        .and_then(|save| {
            let _ = Timing::disabled("construct", |scope| {
                construct_bytes(
                    viewed.policy().construction(),
                    &viewed.policy().construction().capacities(),
                    b"never written",
                    &mut save.sink(),
                    scope.child("content"),
                )
            });
            save.finish()
        })
        .is_err());

    // The wrong binding is refused as for a local open.
    assert!(Handles::open_remote(&file, b"another-binding", CURSOR).is_err());
    drop((viewed, view));
}

#[test]
fn a_changed_file_fails_its_reads_changed_and_sends_nothing_more() {
    let temp = Temp::new("remote-changed");
    let handles = Handles::create(
        config(temp.join("store")),
        StoragePolicy::frozen_default(),
        &HistoryCatalogConfig {
            binding_key: BINDING.to_vec(),
            cursor_key: CURSOR,
            incarnation: 3,
        },
    )
    .unwrap();
    let storage = Storage::new(handles.storage.clone()).unwrap();
    let first = pattern(200_000, 5);
    let second = pattern(2_500_000, 6);
    let (first_id, second_id) = (save(&storage, &first), save(&storage, &second));
    let pushed = handles.snapshot(&temp.join("pushed")).unwrap();
    let source = Bytes::new(std::fs::read(&pushed.path).unwrap());
    assert!(source.bytes() > 2 * MAX_REQUEST, "{}", source.bytes());
    // a small cache, so the second object is not read ahead into it
    let file = RemoteFile::new(source.clone(), MAX_REQUEST);
    let view = Handles::open_remote(&file, BINDING, CURSOR).unwrap();
    let viewed = Storage::new(view.storage.clone()).unwrap();
    assert_eq!(read(&viewed, first_id, first.len() as u64).unwrap(), first);

    source.changed.store(true, Ordering::Relaxed);
    let failures = file.failures();
    assert!(read(&viewed, second_id, second.len() as u64).is_err());
    let failure = file.failure_since(failures).expect("a kept failure");
    assert_eq!(failure.kind, RemoteFailureKind::Changed);
    // Once changed, a miss fails without another request.
    let sent = source.requests.load(Ordering::Relaxed);
    assert!(read(&viewed, second_id, second.len() as u64).is_err());
    assert_eq!(source.requests.load(Ordering::Relaxed), sent);
}

#[test]
fn missing_blocks_merge_into_requests_of_at_most_one_mebibyte() {
    let most = MAX_REQUEST / BLOCK;
    let none = |_: u64| false;
    // one missing block reads ahead to a whole request
    assert_eq!(
        plan(3, 3, 100, &none),
        vec![Run {
            first: 3,
            count: most
        }]
    );
    // ... but not past the file's end
    assert_eq!(plan(3, 3, 5, &none), vec![Run { first: 3, count: 2 }]);
    // ... nor into a present block
    assert_eq!(
        plan(3, 3, 100, &|b| b == 6),
        vec![Run { first: 3, count: 3 }]
    );
    // a present block splits a read's misses; only the last run reads ahead
    assert_eq!(
        plan(0, 4, 100, &|b| b == 2),
        vec![
            Run { first: 0, count: 2 },
            Run {
                first: 3,
                count: most
            }
        ]
    );
    // a long read is cut into whole requests
    assert_eq!(
        plan(0, 2 * most, 1000, &none),
        vec![
            Run {
                first: 0,
                count: most
            },
            Run {
                first: most,
                count: most
            },
            Run {
                first: 2 * most,
                count: most
            },
        ]
    );
    // everything present: no request
    assert!(plan(0, 9, 100, &|_| true).is_empty());
}

#[test]
fn the_cache_keeps_at_most_its_bound_and_answers_from_memory() {
    let source = Bytes::new(pattern(10 * MAX_REQUEST as usize + 1234, 9));
    let cache = BlockCache::new(2 * MAX_REQUEST);
    let mut out = vec![0; 5000];
    // a read across a block boundary is one request
    let offset = BLOCK - 100;
    cache.read(source.as_ref(), offset, &mut out).unwrap();
    assert_eq!(out, source.data[offset as usize..offset as usize + 5000]);
    assert_eq!(source.requests.load(Ordering::Relaxed), 1);
    cache.read(source.as_ref(), offset, &mut out).unwrap();
    assert_eq!(source.requests.load(Ordering::Relaxed), 1);
    // reading the whole file keeps no more than the bound
    let mut at = 0;
    while at < source.bytes() {
        let mut page = vec![0; 4096.min((source.bytes() - at) as usize)];
        cache.read(source.as_ref(), at, &mut page).unwrap();
        assert_eq!(page, source.data[at as usize..at as usize + page.len()]);
        at += page.len() as u64;
    }
    let stats = cache.stats();
    assert!(stats.resident <= 2 * MAX_REQUEST, "{stats:?}");
    assert_eq!(stats.capacity, 2 * MAX_REQUEST);
    // the last, partial block is read to the file's end
    let mut tail = vec![0; 100];
    cache
        .read(source.as_ref(), source.bytes() - 100, &mut tail)
        .unwrap();
    assert_eq!(tail, source.data[source.data.len() - 100..]);
}
