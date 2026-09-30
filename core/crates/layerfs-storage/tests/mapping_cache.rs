//! Real StoreProvider proof for navigation owners and retained PageCache state.
//!
//! Expected bytes/canonical IDs come from independent finite v1 framing, never
//! from a candidate builder/read. Authentication and grouped acquisition delegate
//! unchanged to the real provider. Allocation evidence covers selected-thread
//! Rust requested layouts; SQLite/zstd native heap, OS cache, RSS and upfront
//! global ReadWindowPermit admission are explicitly outside this proof.

mod support;

#[path = "support/allocation_observer.rs"]
mod allocation_observer;
#[path = "../../layerfs-content/tests/support/cache_oracle.rs"]
mod cache_oracle;

use std::cell::RefCell;
use std::mem::size_of;

use layerfs_content::file::mapping::{
    ChildDescriptor, ExtentSlice, PageCache, RangeCursor, ReadCounters,
};
use layerfs_content::{
    AuthenticatedObjects, ContentError, ContentResult, FinalizedObject, ObjectId, ObjectRole,
};
use layerfs_storage::{Store, StoreProvider};
use layerfs_telemetry::timer::TimingScope;

use allocation_observer::{Observation, Pause};
use cache_oracle::{Fixture, FIRST_WAVE_BYTES, LEAF_CANONICAL_BYTES, ROOT_CANONICAL_BYTES};
use support::{create_store, disabled, TempDir};

#[global_allocator]
static ALLOCATOR: allocation_observer::ObservedSystem = allocation_observer::ObservedSystem;

#[derive(Clone, Copy, Debug)]
struct Capacity {
    id: ObjectId,
    length: usize,
    capacity: usize,
}

struct ObservedProvider<'a> {
    inner: StoreProvider<'a>,
    fixture: &'a Fixture,
    navigation: RefCell<Vec<Vec<ObjectId>>>,
    capacities: RefCell<Vec<Capacity>>,
}

impl<'a> ObservedProvider<'a> {
    fn new(store: &'a Store, fixture: &'a Fixture) -> Self {
        Self {
            inner: StoreProvider::new(store),
            fixture,
            navigation: RefCell::new(Vec::with_capacity(8)),
            capacities: RefCell::new(Vec::with_capacity(128)),
        }
    }

    fn observe(&self, ids: &[ObjectId], values: Vec<Vec<u8>>) -> ContentResult<Vec<Vec<u8>>> {
        // Only independent observer bookkeeping is excluded. The provider call
        // already completed under capture, and its returned owners remain live.
        let _pause = Pause::new();
        assert_eq!(values.len(), ids.len());
        for (id, value) in ids.iter().zip(&values) {
            assert_eq!(value.as_slice(), self.fixture.canonical(*id));
        }
        if ids.iter().all(|id| {
            matches!(
                self.fixture.role(*id),
                ObjectRole::ExtentLeaf | ObjectRole::ExtentBranch
            )
        }) {
            self.navigation.borrow_mut().push(ids.to_vec());
            for (id, value) in ids.iter().zip(&values) {
                assert!(value.capacity() <= 8192, "actual mapping owner capacity");
                self.capacities.borrow_mut().push(Capacity {
                    id: *id,
                    length: value.len(),
                    capacity: value.capacity(),
                });
            }
        }
        Ok(values)
    }
}

impl AuthenticatedObjects for ObservedProvider<'_> {
    fn read_canonical_batch(&self, ids: &[ObjectId]) -> ContentResult<Vec<Vec<u8>>> {
        self.observe(ids, self.inner.read_canonical_batch(ids)?)
    }

    fn read_canonical_batch_scoped(
        &self,
        ids: &[ObjectId],
        scope: TimingScope<'_>,
    ) -> ContentResult<Vec<Vec<u8>>> {
        self.observe(ids, self.inner.read_canonical_batch_scoped(ids, scope)?)
    }
}

fn save_fixture(store: &Store, fixture: &Fixture) {
    let order = std::iter::once(fixture.payload_id)
        .chain(fixture.leaves.iter().copied())
        .chain(fixture.dummy.iter().copied())
        .chain([fixture.state.mapping_root, fixture.file_root]);
    let outcome = disabled(|scope| {
        let mut save = store.begin_save(scope.child("storage.begin"))?;
        for id in order {
            let role = fixture.role(id);
            let references = match role {
                ObjectRole::Chunk => Vec::new(),
                ObjectRole::ExtentLeaf => vec![fixture.payload_id; 128],
                ObjectRole::ExtentBranch => fixture.leaves.clone(),
                ObjectRole::FileState => vec![fixture.state.mapping_root],
                _ => unreachable!("fixture contains only file roles"),
            };
            let object = FinalizedObject::new(role, fixture.canonical(id).to_vec())?
                .with_references(references);
            assert_eq!(object.id(), id, "independent canonical identity");
            save.accept(object)?;
        }
        save.finish(scope.child("storage.finish"))
    })
    .expect("real Store save/publication of independent valid fixture");
    assert_eq!(outcome.inserted, fixture.objects.len() as u64);
}

fn retained(cache: &PageCache, fixture: &Fixture) -> usize {
    fixture
        .mapping_keys()
        .filter(|(id, root)| cache.get(*id, *root).is_some())
        .count()
}

fn read_segments(
    provider: &ObservedProvider<'_>,
    fixture: &Fixture,
    cache: &mut PageCache,
    output: &mut Vec<u8>,
    retained_limit: usize,
) -> ReadCounters {
    disabled(|scope| {
        let mut cursor = RangeCursor::new(provider, fixture.state, cache, scope)?;
        cursor.read_segment(0..FIRST_WAVE_BYTES as u64, output, cache)?;
        assert_eq!(output.as_slice(), &fixture.expected[..FIRST_WAVE_BYTES]);
        assert!(retained(cache, fixture) <= retained_limit);
        let counters = cursor.read_segment(
            FIRST_WAVE_BYTES as u64..fixture.state.logical_len,
            output,
            cache,
        )?;
        assert_eq!(output.as_slice(), fixture.expected.as_slice());
        assert!(retained(cache, fixture) <= retained_limit);
        Ok::<_, ContentError>(counters)
    })
    .expect("ascending mapping segments through real StoreProvider")
}

fn cache_resource_observation(fixture: &Fixture) {
    let encoded = [LEAF_CANONICAL_BYTES, ROOT_CANONICAL_BYTES];
    let decoded = [
        128 * size_of::<ExtentSlice>(),
        65 * size_of::<ChildDescriptor>(),
    ];
    let observation = Observation::start(encoded, decoded);
    let mut cache = PageCache::default();
    for (id, root) in fixture.mixed_prefill() {
        cache
            .insert(id, root, fixture.canonical(id).to_vec())
            .expect("each real requested owner fits the cache");
    }
    let held = observation.stop();
    let expected_encoded: usize = fixture
        .mixed_prefill()
        .map(|(id, _)| fixture.canonical(id).len())
        .sum();
    assert_eq!(retained(&cache, fixture), 64);
    assert!(!held.overflow, "fixed external observer ledger overflowed");
    assert_eq!(held.live[0], expected_encoded);
    assert_eq!(held.live[1], 0, "validation decoded owner released");
    assert!(held.peak[0] <= 64 * 8192, "encoded cache owners: {held:?}");
    assert!(held.peak[1] <= decoded.into_iter().max().unwrap());
    assert!(
        held.peak[2] <= 16 * 1024,
        "cache table requested layouts: {held:?}"
    );
    assert!(held.peak_total <= 64 * 8192 + 16 * 1024 + decoded.into_iter().max().unwrap());
    drop(cache);
    let released = observation.released();
    assert_eq!(
        released.live_total(),
        0,
        "exact observed cache release: {released:?}"
    );
    eprintln!(
        "R1b diagnostic cache-prefill: requested={held:?}; released={released:?}; \
         PageCache stack metadata={} bytes; exact-Layout-size classes may collide; \
         fixtures/oracle/output/external observer excluded; allocator metadata/native heap/RSS unavailable",
        size_of::<PageCache>(),
    );
}

fn mixed_real_provider_observation(store: &Store, fixture: &Fixture) {
    let provider = ObservedProvider::new(store, fixture);
    let mut output = Vec::with_capacity(fixture.expected.len());
    let mut cache = PageCache::default();
    for (id, root) in fixture.mixed_prefill() {
        cache
            .insert(id, root, fixture.canonical(id).to_vec())
            .unwrap();
    }
    assert_eq!(retained(&cache, fixture), 64);
    let counters = read_segments(&provider, fixture, &mut cache, &mut output, 64);
    let calls = provider.navigation.borrow();
    assert_eq!(
        calls[0],
        fixture.leaves[1..32],
        "one hit and31 grouped misses"
    );
    assert!(calls[0].iter().all(|id| *id != fixture.leaves[0]));
    assert_eq!(counters.max_node_batch, 32);
    assert_eq!(provider.inner.connection_opens(), 1);
    assert!(calls.iter().all(|ids| ids.len() <= 32));
    for demand in provider.capacities.borrow().iter() {
        assert_eq!(demand.length, fixture.canonical(demand.id).len());
        assert!(demand.length <= demand.capacity && demand.capacity <= 8192);
    }
}

fn provider_requested_allocation_observation(store: &Store, fixture: &Fixture) {
    let provider = ObservedProvider::new(store, fixture);
    let mut output = Vec::with_capacity(fixture.expected.len());
    let mut cache = PageCache::default();
    let observation = Observation::start(
        [LEAF_CANONICAL_BYTES, ROOT_CANONICAL_BYTES],
        [
            128 * size_of::<ExtentSlice>(),
            65 * size_of::<ChildDescriptor>(),
        ],
    );
    read_segments(&provider, fixture, &mut cache, &mut output, 64);
    let during_read = observation.stop();
    assert!(
        !during_read.overflow,
        "real provider requested-allocation ledger"
    );
    assert_eq!(provider.inner.connection_opens(), 1);
    assert!(during_read.allocation_events > 0);
    drop(provider);
    let after_provider = observation.stop();
    assert!(
        after_provider.live_total() <= 64 * 8192 + 16 * 1024,
        "remaining Rust owners after provider drop must fit retained cache: {after_provider:?}"
    );
    drop(cache);
    let released = observation.released();
    assert_eq!(
        released.live_total(),
        0,
        "actual Rust owner release: {released:?}"
    );
    eprintln!(
        "R1b diagnostic real-provider-read: requested={during_read:?}; \
         after-provider-drop={after_provider:?}; released={released:?}; \
         includes provider Rust session/decoded/batch/cache overlap; \
         fixtures/preallocated output/observer excluded; no upfront permit/native heap/RSS claim"
    );
}

#[test]
fn real_store_cache_crossing_preserves_independent_bytes_and_requested_owners() {
    let fixture = cache_oracle::wide();
    let directory = TempDir::new("r1b_mapping_cache");
    let store = create_store(&directory.store_path("objects"));
    save_fixture(&store, &fixture);

    cache_resource_observation(&fixture);
    mixed_real_provider_observation(&store, &fixture);
    for limit in [0usize, 1, 31, 32, 64] {
        let provider = ObservedProvider::new(&store, &fixture);
        let mut cache = PageCache::bounded(limit);
        let mut output = Vec::with_capacity(fixture.expected.len());
        let counters = read_segments(&provider, &fixture, &mut cache, &mut output, limit.max(1));
        let calls = provider.navigation.borrow();
        assert_eq!(calls[0], [fixture.state.mapping_root]);
        assert_eq!(calls[1], fixture.leaves[..32]);
        assert_eq!(counters.max_node_batch, 32);
        assert!(calls.iter().all(|ids| ids.len() <= 32));
        assert_eq!(provider.inner.connection_opens(), 1);
    }

    let mut cache = PageCache::bounded(1);
    let root = fixture.state.mapping_root;
    cache
        .insert(root, true, fixture.canonical(root).to_vec())
        .unwrap();
    let mut excess_owner = Vec::with_capacity(8193);
    excess_owner.extend_from_slice(fixture.canonical(root));
    let actual = excess_owner.capacity();
    assert_eq!(
        cache.insert(root, true, excess_owner),
        Err(ContentError::BoundedCapacityExceeded {
            what: "mapping.page_capacity",
            limit: 8192,
            actual: actual as u64,
        })
    );
    assert_eq!(cache.get(root, true), Some(fixture.canonical(root)));
    assert_eq!(retained(&cache, &fixture), 1);
    drop(cache);
    provider_requested_allocation_observation(&store, &fixture);
}
