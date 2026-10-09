//! A regular file's length is the Store's answer about its immutable
//! content root. The canonical client remembers it in the cache's own
//! allowance: the provider is asked once per remembered root, a memory-only
//! client answers what is remembered and nothing else, and an allowance that
//! cannot hold an answer only costs another demand. Public API over a
//! content-built base; the length provider counts its demands.
mod common;
use common::{fixture, Fixture, Store};
use layerfs_content::ObjectId;
use layerfs_workspace::{
    BaseView, CanonicalCache, CanonicalClient, ClientWork, FileLengths, WorkspaceResult,
};
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};

/// The two regular files of the fixture and their lengths.
const SMALL: (u64, u64) = (2, 10);
const LARGE: (u64, u64) = (8, 400_000);
/// What one remembered length is charged: its value and the bookkeeping
/// allowance of any cache entry.
const LENGTH_CHARGE: usize = 8 + 256;

struct Counting {
    store: Store,
    asked: AtomicU64,
}
impl FileLengths for Counting {
    fn file_length(&self, id: ObjectId) -> WorkspaceResult<u64> {
        self.asked.fetch_add(1, Ordering::Relaxed);
        self.store.file_length(id)
    }
}
struct Bench {
    f: Fixture,
    lengths: Arc<Counting>,
    cache: Arc<CanonicalCache>,
    base: BaseView,
}
impl Bench {
    fn new(cache_bytes: usize) -> Self {
        let f = fixture();
        let lengths = Arc::new(Counting {
            store: f.store.clone(),
            asked: AtomicU64::new(0),
        });
        let cache = Arc::new(CanonicalCache::new(cache_bytes));
        let client = Arc::new(CanonicalClient::with_cache(
            Arc::new(f.store.clone()),
            Some(lengths.clone()),
            cache.clone(),
        ));
        let base = BaseView::open(client, f.root, f.scope).unwrap();
        Self {
            f,
            lengths,
            cache,
            base,
        }
    }
    fn asked(&self) -> u64 {
        self.lengths.asked.load(Ordering::Relaxed)
    }
    fn demand(&self) -> u64 {
        self.f.store.demand.load(Ordering::Relaxed)
    }
    fn work(&self) -> ClientWork {
        self.cache.diagnostics().unwrap()
    }
    fn length(&self, serial: u64) -> u64 {
        self.base.stat(serial).unwrap().logical_len
    }
}

#[test]
fn a_file_length_is_asked_once_and_then_answered_from_memory() {
    let b = Bench::new(1024 * 1024);
    let resident = b
        .base
        .with_client(Arc::new(CanonicalClient::resident(b.cache.clone())));
    // Nothing is remembered yet: the memory-only client has no answer and
    // asks nobody.
    assert!(resident.stat(SMALL.0).is_err());
    assert_eq!((b.asked(), b.work().file_lengths), (0, 0));

    assert_eq!(b.length(SMALL.0), SMALL.1);
    assert_eq!((b.asked(), b.work().file_lengths), (1, 1));
    let (charged, demand) = (b.work().charged_cache_bytes, b.demand());
    for _ in 0..3 {
        assert_eq!(b.length(SMALL.0), SMALL.1);
    }
    // No provider demand of either kind, and nothing more is charged.
    assert_eq!((b.asked(), b.demand()), (1, demand));
    assert_eq!(b.work().charged_cache_bytes, charged);

    // The memory-only client answers the remembered length, from memory.
    assert_eq!(resident.stat(SMALL.0).unwrap().logical_len, SMALL.1);
    assert_eq!((b.asked(), b.demand()), (1, demand));
    // A file whose length was never asked is not answered by it.
    assert!(resident.stat(LARGE.0).is_err());
    assert_eq!(b.asked(), 1);

    assert_eq!(b.length(LARGE.0), LARGE.1);
    assert_eq!(resident.stat(LARGE.0).unwrap().logical_len, LARGE.1);
    let work = b.work();
    assert_eq!((b.asked(), work.file_lengths, work.evictions), (2, 2, 0));
    // The answers are charged to the one allowance, beside the objects.
    assert!(work.charged_cache_bytes >= 2 * LENGTH_CHARGE + work.cached_objects * 256);
    println!(
        "FILE_LENGTHS asked=2 for 2 files and 9 stats; remembered={} charged={} objects={}",
        work.file_lengths, work.charged_cache_bytes, work.cached_objects
    );
}

#[test]
fn an_allowance_that_holds_no_answer_asks_every_time_and_refuses_nothing() {
    for allowance in [0, LENGTH_CHARGE - 1] {
        let b = Bench::new(allowance);
        for round in 1..=3 {
            assert_eq!(b.length(SMALL.0), SMALL.1);
            assert_eq!(b.length(LARGE.0), LARGE.1);
            assert_eq!(b.asked(), 2 * round);
        }
        let work = b.work();
        assert_eq!(
            (
                work.file_lengths,
                work.cached_objects,
                work.charged_cache_bytes
            ),
            (0, 0, 0)
        );
    }
}

#[test]
fn an_evicted_length_is_asked_again_and_the_allowance_is_never_exceeded() {
    // Room for exactly one remembered length and for no object.
    let b = Bench::new(LENGTH_CHARGE);
    assert_eq!(b.length(SMALL.0), SMALL.1);
    assert_eq!((b.asked(), b.work().file_lengths), (1, 1));
    // The other file's answer takes the place of the first.
    assert_eq!(b.length(LARGE.0), LARGE.1);
    assert_eq!(b.length(LARGE.0), LARGE.1);
    assert_eq!((b.asked(), b.work().file_lengths), (2, 1));
    // The evicted answer is asked again, correctly, and remembered again.
    assert_eq!(b.length(SMALL.0), SMALL.1);
    assert_eq!(b.length(SMALL.0), SMALL.1);
    let work = b.work();
    assert_eq!(
        (
            b.asked(),
            work.file_lengths,
            work.evictions,
            work.cached_objects,
            work.charged_cache_bytes
        ),
        (3, 1, 2, 0, LENGTH_CHARGE)
    );
}
