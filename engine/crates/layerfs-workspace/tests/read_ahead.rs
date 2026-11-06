//! layerfs-project #97: a sequential base read of committed chunked content
//! fetches the next 8 MiB window's chunks in one upstream demand, within a
//! cache of at least two windows; random reads, small files and a small
//! cache fetch nothing ahead. Public API over a content-built base, with an
//! upstream that records each demand.
mod common;
use common::{fixture_with, Fixture, Store};
use layerfs_content::{AuthenticatedObjects, ContentResult, ObjectId};
use layerfs_workspace::{BaseView, CanonicalCache, CanonicalClient};
use std::sync::{Arc, Mutex};

const MIB: usize = 1024 * 1024;
/// One kernel read request.
const READ: u32 = 128 * 1024;
/// The large file, serial 8: non-repeating, so no chunk is shared.
const LARGE: usize = 32 * MIB;
/// The read-ahead window the client fetches.
const WINDOW: usize = 8 * MIB;

/// The fixture's store, recording the ids and canonical bytes of each demand.
#[derive(Clone)]
struct Recording {
    store: Store,
    demands: Arc<Mutex<Vec<(usize, usize)>>>,
}
impl AuthenticatedObjects for Recording {
    fn read_canonical_batch(&self, ids: &[ObjectId]) -> ContentResult<Vec<Vec<u8>>> {
        let values = self.store.read_canonical_batch(ids)?;
        let bytes = values.iter().map(Vec::len).sum();
        self.demands.lock().unwrap().push((ids.len(), bytes));
        Ok(values)
    }
}

fn noise(length: usize) -> Vec<u8> {
    let mut state = 0x9e37_79b9_7f4a_7c15_u64;
    (0..length)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            (state >> 24) as u8
        })
        .collect()
}

fn open(f: &Fixture, cache: usize) -> (BaseView, Recording, Arc<CanonicalCache>) {
    let recording = Recording {
        store: f.store.clone(),
        demands: Arc::new(Mutex::new(Vec::new())),
    };
    let cache = Arc::new(CanonicalCache::new(cache));
    let client = Arc::new(CanonicalClient::with_cache(
        Arc::new(recording.clone()),
        Some(Arc::new(f.store.clone())),
        cache.clone(),
    ));
    let base = BaseView::open(client, f.root, f.scope).unwrap();
    recording.demands.lock().unwrap().clear();
    (base, recording, cache)
}

fn read(base: &BaseView, serial: u64, offset: u64, length: u32) -> Vec<u8> {
    let mut out = Vec::new();
    base.plan_read(serial, offset, length)
        .unwrap()
        .emit(&mut out)
        .unwrap();
    out
}

/// Reads all of serial 8 in kernel-sized windows, checking every byte.
fn sequential(base: &BaseView, bytes: &[u8]) {
    let mut offset = 0;
    while offset < bytes.len() {
        let piece = read(base, 8, offset as u64, READ);
        assert!(!piece.is_empty());
        assert_eq!(piece, bytes[offset..offset + piece.len()], "at {offset}");
        offset += piece.len();
    }
}

#[test]
fn a_sequential_read_fetches_each_window_in_one_demand() {
    let f = fixture_with(noise(LARGE));
    let (base, recording, cache) = open(&f, 64 * MIB);
    sequential(&base, &f.bytes);
    let demands = recording.demands.lock().unwrap().clone();
    let largest = demands.iter().map(|(_, bytes)| *bytes).max().unwrap();
    // The windows are fetched whole: about LARGE / WINDOW chunk demands,
    // each about one window, plus the mapping pages on the way and the
    // first read's own wave.
    let windows = demands
        .iter()
        .filter(|(_, bytes)| *bytes > WINDOW / 2)
        .count();
    assert_eq!(windows, LARGE.div_ceil(WINDOW), "{demands:?}");
    assert!(largest <= WINDOW + 2 * READ as usize + MIB, "{demands:?}");
    assert!(
        demands.len() <= 3 * LARGE.div_ceil(WINDOW) + 4,
        "{demands:?}"
    );
    // Every chunk is demanded once: what was fetched ahead was not evicted
    // before it was read.
    let work = cache.diagnostics().unwrap();
    assert_eq!(work.evictions, 0);
    let served: usize = demands.iter().map(|(_, bytes)| *bytes).sum();
    assert!(served < LARGE + LARGE / 16, "served {served} of {LARGE}");
    println!(
        "READ_AHEAD sequential demands={} upstream_batches={} served={served}",
        demands.len(),
        work.upstream_batches
    );
}

#[test]
fn random_and_small_reads_fetch_nothing_ahead() {
    let f = fixture_with(noise(LARGE));
    let (base, recording, _) = open(&f, 64 * MIB);
    // Descending windows: none continues the one before it.
    for n in (0..16_u64).rev() {
        let offset = n * 2 * MIB as u64 + 4096;
        let piece = read(&base, 8, offset, READ);
        assert_eq!(piece, f.bytes[offset as usize..][..READ as usize]);
    }
    let served: usize = recording.demands.lock().unwrap().iter().map(|d| d.1).sum();
    // Each window costs its own chunks and pages, never a window ahead.
    assert!(
        served < 16 * (READ as usize + 160 * 1024),
        "served {served}"
    );
    // A small file is one whole-file object: nothing to fetch ahead.
    recording.demands.lock().unwrap().clear();
    assert_eq!(read(&base, 2, 0, READ), b"original\0\xff");
    assert_eq!(read(&base, 2, 10, READ), b"");
    let demands = recording.demands.lock().unwrap().clone();
    assert!(
        demands.iter().all(|(_, bytes)| *bytes < 4096),
        "{demands:?}"
    );
}

#[test]
fn a_cache_under_two_windows_reads_without_fetching_ahead() {
    let f = fixture_with(noise(LARGE));
    let (base, recording, cache) = open(&f, 2 * WINDOW - 1);
    sequential(&base, &f.bytes);
    let demands = recording.demands.lock().unwrap().clone();
    // Read waves of at most 32 chunks, never a window.
    assert!(demands.iter().all(|(ids, _)| *ids <= 32), "{demands:?}");
    assert!(cache.diagnostics().unwrap().charged_cache_bytes < 2 * WINDOW);
}
