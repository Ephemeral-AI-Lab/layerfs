//! The shared immutable cache through its public client: exact counts,
//! exact least-recently-used eviction and accounting, authentication of
//! every fetched object, and what a base lookup acquires.
//!
//! The upstream here is the test's own and records every demand. Counts
//! only, except one labelled host timing that asserts nothing.
mod common;
use common::name;
use layerfs_content::{
    file::mapping::encode_chunk_object, policy::WHOLE_FILE_CANONICAL_OVERHEAD,
    AuthenticatedObjects, ContentError, ContentResult, FinalizedObject, ObjectId, ObjectRole,
};
use layerfs_workspace::{BaseView, CanonicalCache, CanonicalClient, ClientWork, FileLengths};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};

/// What one retained object and one remembered length are charged.
const OBJECT_CHARGE: usize = 256;
const LENGTH_CHARGE: usize = 8 + 256;

/// Serves the objects it was given, records each batch it was asked for,
/// and can be told to answer one identity with other bytes.
#[derive(Default)]
struct Upstream {
    objects: Mutex<BTreeMap<ObjectId, Vec<u8>>>,
    batches: Mutex<Vec<Vec<ObjectId>>>,
    wrong: Mutex<Option<ObjectId>>,
    lengths: Mutex<Vec<ObjectId>>,
}
impl Upstream {
    /// A new object of `length` bytes, distinct for each `tag`.
    fn object(&self, tag: u8, length: usize) -> ObjectId {
        let bytes: Vec<u8> = (0..length).map(|n| tag ^ (n as u8)).collect();
        let id = ObjectId::for_bytes(&bytes);
        self.objects.lock().unwrap().insert(id, bytes);
        id
    }
    fn bytes(&self, id: ObjectId) -> Vec<u8> {
        self.objects.lock().unwrap()[&id].clone()
    }
    /// The batches demanded since the last call.
    fn demanded(&self) -> Vec<Vec<ObjectId>> {
        std::mem::take(&mut *self.batches.lock().unwrap())
    }
}
impl AuthenticatedObjects for Upstream {
    fn read_canonical_batch(&self, ids: &[ObjectId]) -> ContentResult<Vec<Vec<u8>>> {
        self.batches.lock().unwrap().push(ids.to_vec());
        let wrong = *self.wrong.lock().unwrap();
        let objects = self.objects.lock().unwrap();
        ids.iter()
            .map(|id| {
                let mut bytes = objects
                    .get(id)
                    .cloned()
                    .ok_or(ContentError::MissingObject)?;
                if wrong == Some(*id) {
                    bytes[0] ^= 1;
                }
                Ok(bytes)
            })
            .collect()
    }
}
impl FileLengths for Upstream {
    fn file_length(&self, id: ObjectId) -> layerfs_workspace::WorkspaceResult<u64> {
        self.lengths.lock().unwrap().push(id);
        Ok(id.to_bytes()[0] as u64)
    }
}
fn client(upstream: &Arc<Upstream>, allowance: usize) -> CanonicalClient {
    CanonicalClient::with_lengths(upstream.clone(), upstream.clone(), allowance)
}
fn work(client: &CanonicalClient) -> ClientWork {
    client.diagnostics().unwrap()
}

#[test]
fn one_demand_asks_upstream_once_for_exactly_what_is_not_held() {
    let upstream = Arc::new(Upstream::default());
    let client = client(&upstream, 1 << 20);
    let ids: Vec<ObjectId> = (0..5_u8)
        .map(|tag| upstream.object(tag, 100 + tag as usize))
        .collect();
    assert_eq!(
        client.read_canonical(ids[1]).unwrap(),
        upstream.bytes(ids[1])
    );
    assert_eq!(
        client.read_canonical(ids[3]).unwrap(),
        upstream.bytes(ids[3])
    );
    assert_eq!(upstream.demanded(), vec![vec![ids[1]], vec![ids[3]]]);
    let before = work(&client);

    // Five identities known together: one upstream batch of the three that
    // are not held, in demand order, and every answer in demand order.
    let answers = client.read_canonical_batch(&ids).unwrap();
    for (id, bytes) in ids.iter().zip(&answers) {
        assert_eq!(*bytes, upstream.bytes(*id));
    }
    assert_eq!(upstream.demanded(), vec![vec![ids[0], ids[2], ids[4]]]);
    let after = work(&client);
    assert_eq!(
        (
            after.cache_hits - before.cache_hits,
            after.cache_misses - before.cache_misses,
            after.upstream_batches - before.upstream_batches,
            after.authenticated_bytes - before.authenticated_bytes,
        ),
        (2, 3, 1, 100 + 102 + 104)
    );
    // Everything held: no upstream demand at all.
    client.read_canonical_batch(&ids).unwrap();
    assert!(upstream.demanded().is_empty());
    assert_eq!(work(&client).cache_hits - after.cache_hits, 5);
    assert_eq!(work(&client).cached_objects, 5);
    assert_eq!(
        work(&client).charged_cache_bytes,
        (100 + 101 + 102 + 103 + 104) + 5 * OBJECT_CHARGE
    );
}

#[test]
fn eviction_is_least_recently_used_and_every_charge_is_exact() {
    let upstream = Arc::new(Upstream::default());
    // Room for exactly three objects of 100 bytes.
    let allowance = 3 * (100 + OBJECT_CHARGE);
    let client = client(&upstream, allowance);
    let [a, b, c, d, e] = [1, 2, 3, 4, 5].map(|tag| upstream.object(tag, 100));
    let read = |id: ObjectId| {
        assert_eq!(client.read_canonical(id).unwrap(), upstream.bytes(id));
        upstream.demanded().len()
    };
    assert_eq!([read(a), read(b), read(c)], [1, 1, 1]);
    assert_eq!(
        (work(&client).charged_cache_bytes, work(&client).evictions),
        (allowance, 0)
    );
    // Using `a` makes `b` the oldest: `d` evicts `b` and nothing else.
    assert_eq!(read(a), 0);
    assert_eq!(read(d), 1);
    assert_eq!(work(&client).evictions, 1);
    assert_eq!([read(c), read(a), read(d)], [0, 0, 0]);
    // The order of use is now c, a, d: `b` returns and evicts `c`, then `e`
    // evicts `a`.
    assert_eq!(read(b), 1);
    assert_eq!(read(e), 1);
    assert_eq!(work(&client).evictions, 3);
    assert_eq!([read(d), read(b), read(e)], [0, 0, 0]);
    assert_eq!([read(c), read(a)], [1, 1]);
    let held = work(&client);
    assert_eq!(
        (
            held.cached_objects,
            held.charged_cache_bytes,
            held.evictions
        ),
        (3, allowance, 5)
    );

    // A remembered length is charged to the same allowance and takes its
    // turn in the same order. Held, oldest first: e, c, a.
    let lengths = || std::mem::take(&mut *upstream.lengths.lock().unwrap());
    assert_eq!(client.file_length(d).unwrap(), d.to_bytes()[0] as u64);
    assert_eq!(lengths(), vec![d]);
    let held = work(&client);
    assert_eq!(
        (
            held.cached_objects,
            held.file_lengths,
            held.charged_cache_bytes,
            held.evictions
        ),
        (2, 1, 2 * (100 + OBJECT_CHARGE) + LENGTH_CHARGE, 6)
    );
    assert_eq!(read(e), 1);
    // Remembered: asked of the provider once, and using it keeps it.
    assert_eq!(client.file_length(d).unwrap(), d.to_bytes()[0] as u64);
    assert!(lengths().is_empty());
    assert_eq!([read(a), read(e)], [0, 0]);
    // Held, oldest first: the length of d, a, e. One more object needs 356
    // bytes and 92 are free: the length goes, and that is room enough.
    assert_eq!(read(b), 1);
    let held = work(&client);
    assert_eq!(
        (
            held.cached_objects,
            held.file_lengths,
            held.charged_cache_bytes,
            held.evictions
        ),
        (3, 0, allowance, 8)
    );
    assert_eq!([read(a), read(e), read(b)], [0, 0, 0]);
    assert_eq!(client.file_length(d).unwrap(), d.to_bytes()[0] as u64);
    assert_eq!(lengths(), vec![d]);
}

#[test]
fn an_evicted_object_is_authenticated_again_and_a_wrong_answer_is_refused() {
    let upstream = Arc::new(Upstream::default());
    let allowance = 2 * (100 + OBJECT_CHARGE);
    let client = client(&upstream, allowance);
    let [a, b, c] = [1, 2, 3].map(|tag| upstream.object(tag, 100));
    assert_eq!(client.read_canonical(a).unwrap(), upstream.bytes(a));
    client.read_canonical(b).unwrap();
    client.read_canonical(c).unwrap();
    upstream.demanded();
    let before = work(&client);
    assert_eq!((before.cached_objects, before.evictions), (2, 1));

    // `a` was evicted. Upstream now answers it with other bytes: the demand
    // is refused, nothing is retained and nothing is counted as authentic.
    *upstream.wrong.lock().unwrap() = Some(a);
    assert_eq!(
        client.read_canonical(a).unwrap_err(),
        ContentError::IdentityMismatch
    );
    assert_eq!(upstream.demanded(), vec![vec![a]]);
    let refused = work(&client);
    assert_eq!(
        (
            refused.cached_objects,
            refused.charged_cache_bytes,
            refused.authenticated_bytes,
            refused.evictions
        ),
        (
            before.cached_objects,
            before.charged_cache_bytes,
            before.authenticated_bytes,
            before.evictions
        )
    );
    // What is held is still served, with no upstream demand.
    assert_eq!(client.read_canonical(c).unwrap(), upstream.bytes(c));
    assert!(upstream.demanded().is_empty());

    // The right answer is fetched, authenticated and retained again.
    *upstream.wrong.lock().unwrap() = None;
    assert_eq!(client.read_canonical(a).unwrap(), upstream.bytes(a));
    assert_eq!(upstream.demanded(), vec![vec![a]]);
    let again = work(&client);
    assert_eq!(again.authenticated_bytes - refused.authenticated_bytes, 100);
    assert_eq!((again.cached_objects, again.evictions), (2, 2));
    assert_eq!(client.read_canonical(a).unwrap(), upstream.bytes(a));
    assert!(upstream.demanded().is_empty());

    // An object charged more than the whole allowance is served every time
    // and never retained; it evicts nothing.
    let large = upstream.object(9, allowance);
    for _ in 0..2 {
        assert_eq!(client.read_canonical(large).unwrap(), upstream.bytes(large));
        assert_eq!(upstream.demanded(), vec![vec![large]]);
    }
    let bypassed = work(&client);
    assert_eq!(
        (
            bypassed.cached_objects,
            bypassed.charged_cache_bytes,
            bypassed.evictions
        ),
        (2, again.charged_cache_bytes, again.evictions)
    );
}

#[test]
fn two_clients_of_one_cache_share_objects_and_keep_their_own_upstreams() {
    let (first, second) = (Arc::new(Upstream::default()), Arc::new(Upstream::default()));
    let id = first.object(7, 300);
    second.object(7, 300);
    let cache = Arc::new(CanonicalCache::new(1 << 20));
    let one = CanonicalClient::with_cache(first.clone(), None, cache.clone());
    let two = CanonicalClient::with_cache(second.clone(), None, cache.clone());
    assert_eq!(one.read_canonical(id).unwrap(), first.bytes(id));
    // The second client finds the object by its identity alone.
    assert_eq!(two.read_canonical(id).unwrap(), first.bytes(id));
    assert_eq!(first.demanded(), vec![vec![id]]);
    assert!(second.demanded().is_empty());
    // A memory-only client reads it too, and nothing it does not hold.
    let resident = CanonicalClient::resident(cache.clone());
    assert_eq!(resident.read_canonical(id).unwrap(), first.bytes(id));
    let absent = second.object(8, 300);
    assert!(resident.read_canonical(absent).is_err());
    assert!(second.demanded().is_empty());
    let shared = cache.diagnostics().unwrap();
    assert_eq!(
        (
            shared.cache_hits,
            shared.cache_misses,
            shared.cached_objects
        ),
        (2, 1, 1)
    );
}

/// Cache acquisitions (hits and misses together) of one call. A page whose
/// decode is kept (#364) is acquired from that cache instead; a decode miss
/// is the canonical acquisition it then makes.
fn acquisitions(client: &CanonicalClient, call: impl FnOnce()) -> u64 {
    let before = work(client);
    call();
    let after = work(client);
    (after.cache_hits + after.cache_misses + after.decoded.hits)
        - (before.cache_hits + before.cache_misses + before.decoded.hits)
}

#[test]
fn a_base_lookup_acquires_only_the_pages_on_its_path() {
    let f = common::fixture_with(Vec::new());
    let client = Arc::new(CanonicalClient::with_lengths(
        Arc::new(f.store.clone()),
        Arc::new(f.store.clone()),
        1 << 20,
    ));
    let base = BaseView::open(client.clone(), f.root, f.scope).unwrap();
    // Everything these calls need is first made resident.
    for _ in 0..2 {
        base.inode(2).unwrap();
        base.child(1, &name("file")).unwrap();
        base.stat(2).unwrap();
        base.stat(3).unwrap();
        base.readlink(3).unwrap();
        base.list(1, None, 16, 4096).unwrap();
    }
    let counted = [
        acquisitions(&client, || {
            base.inode(2).unwrap();
        }),
        acquisitions(&client, || {
            base.child(1, &name("file")).unwrap();
        }),
        acquisitions(&client, || {
            base.stat(2).unwrap();
        }),
        acquisitions(&client, || {
            base.stat(3).unwrap();
        }),
        acquisitions(&client, || {
            base.readlink(3).unwrap();
        }),
        acquisitions(&client, || {
            base.list(1, None, 16, 4096).unwrap();
        }),
    ];
    println!(
        "BASE_ACQUISITIONS inode={} child={} stat_file={} stat_symlink={} readlink={} list={}",
        counted[0], counted[1], counted[2], counted[3], counted[4], counted[5]
    );
    // The filesystem root is the one checked at open and is not acquired
    // again. Each call acquires the one inode-table page of this small tree
    // for each inode it resolves, one directory page, one symlink target.
    // Portable mode and mtime are remembered by metadata root: none.
    assert_eq!(counted, [1, 3, 1, 2, 2, 2]);
}

#[test]
fn portable_metadata_is_decoded_once_remembered_by_root_and_evicted_in_turn() {
    const PORTABLE_CHARGE: usize = 16 + 256;
    struct Lengths(common::Store, std::sync::atomic::AtomicU64);
    impl FileLengths for Lengths {
        fn file_length(&self, id: ObjectId) -> layerfs_workspace::WorkspaceResult<u64> {
            self.1.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            self.0.file_length(id)
        }
    }
    let f = common::fixture_with(Vec::new());
    // The canonical allowance alone: no decoded page is kept (#364), so
    // every page a stat reads is an acquisition of this allowance.
    let open = |allowance: usize| {
        let lengths = Arc::new(Lengths(f.store.clone(), Default::default()));
        let client = Arc::new(CanonicalClient::with_cache(
            Arc::new(f.store.clone()),
            Some(lengths.clone()),
            Arc::new(CanonicalCache::with_decoded(allowance, 0)),
        ));
        let base = BaseView::open(client.clone(), f.root, f.scope).unwrap();
        (client, base, lengths)
    };
    let (client, base, _) = open(1 << 20);
    let demand = || f.store.demand.load(std::sync::atomic::Ordering::Relaxed);

    // The first stat of a file reads the attribute page and three objects
    // for each of mode and mtime, once, and remembers the checked answer.
    let (start, asked) = (work(&client), demand());
    let first = base.stat(2).unwrap();
    let read = work(&client);
    assert_eq!(demand() - asked, 1 + 7);
    assert_eq!(
        (read.portable_metadata_reads, read.portable_metadata),
        (1, 1)
    );
    let objects = read.cached_objects - start.cached_objects;
    assert_eq!(objects, 1 + 7);
    assert_eq!(
        read.charged_cache_bytes - start.charged_cache_bytes,
        (read.authenticated_bytes - start.authenticated_bytes) as usize
            + objects * OBJECT_CHARGE
            + LENGTH_CHARGE
            + PORTABLE_CHARGE
    );
    // Again: the inode page and nothing else, the same answer.
    assert_eq!(
        acquisitions(&client, || assert_eq!(base.stat(2).unwrap(), first)),
        1
    );
    // The large file has the same metadata root and kind: remembered.
    assert_eq!(
        acquisitions(&client, || {
            let other = base.stat(8).unwrap();
            assert_eq!(other.metadata, first.metadata);
        }),
        1
    );
    // A symlink and a directory have metadata roots of their own.
    base.stat(3).unwrap();
    base.stat(1).unwrap();
    let all = work(&client);
    assert_eq!((all.portable_metadata_reads, all.portable_metadata), (3, 3));
    assert_eq!(all.evictions, 0);

    // An allowance that holds the objects of one stat and not of two:
    // reading a second inode's metadata evicts the first answer in its
    // turn, and the next stat of the first reads its objects again.
    let (small, base, lengths) = open(read.charged_cache_bytes - start.charged_cache_bytes + 64);
    let file = base.stat(2).unwrap();
    assert_eq!(file, first);
    assert_eq!(work(&small).portable_metadata, 1);
    let link = base.stat(3).unwrap();
    let pressed = work(&small);
    assert!(pressed.evictions > 0);
    assert!(
        pressed.charged_cache_bytes <= read.charged_cache_bytes - start.charged_cache_bytes + 64
    );
    assert_eq!(pressed.portable_metadata_reads, 2);
    assert_eq!(base.stat(2).unwrap(), first);
    assert_eq!(base.stat(3).unwrap(), link);
    let again = work(&small);
    assert!(again.portable_metadata_reads > pressed.portable_metadata_reads);
    assert!(again.charged_cache_bytes <= read.charged_cache_bytes - start.charged_cache_bytes + 64);
    // Entries held are entries inserted less entries evicted, of all kinds.
    assert_eq!(
        (again.cached_objects + again.file_lengths + again.portable_metadata) as u64,
        again.cache_misses
            + lengths.1.load(std::sync::atomic::Ordering::Relaxed)
            + again.portable_metadata_reads
            - again.evictions
    );

    // With no allowance nothing is remembered and every stat reads.
    let (none, base, _) = open(0);
    for _ in 0..2 {
        assert_eq!(
            acquisitions(&none, || assert_eq!(base.stat(2).unwrap(), first)),
            8
        );
    }
    assert_eq!(
        (
            work(&none).portable_metadata,
            work(&none).portable_metadata_reads
        ),
        (0, 2)
    );
}

/// Host timing of a hit, for the diagnosis only: it asserts nothing about
/// time. The cache holds 8,192 objects of one size; every hit is of a
/// different object than the one before.
/// One finalized object of `role` whose canonical bytes are `length` long.
fn finalized(role: ObjectRole, tag: u8, length: usize) -> FinalizedObject {
    let overhead = encode_chunk_object(&[0]).unwrap().len() - 1;
    let value: Vec<u8> = (0..length - overhead).map(|n| tag ^ (n as u8)).collect();
    let object = FinalizedObject::new(role, encode_chunk_object(&value).unwrap()).unwrap();
    assert_eq!(object.canonical_len(), length);
    object
}

#[test]
fn constructed_objects_are_staged_by_role_within_a_bound_and_placed_without_a_fetch() {
    // A Commit may place one eighth of the allowance.
    let allowance = 8 * 4096;
    let cache = Arc::new(CanonicalCache::new(allowance));
    let upstream = Arc::new(Upstream::default());
    let reader = CanonicalClient::with_cache(upstream.clone(), None, cache.clone());

    // What is kept: every role but a file's payload; a chunk only when it
    // is no longer than an entry's own bookkeeping charge; each object once.
    // Of a whole file, its length: canonical bytes less the fixed overhead.
    let mut staged = cache.constructed().unwrap();
    assert!(staged.is_empty());
    let whole = finalized(ObjectRole::WholeFile, 1, 100);
    let chunk = finalized(ObjectRole::Chunk, 2, OBJECT_CHARGE + 1);
    let value = finalized(ObjectRole::Chunk, 3, OBJECT_CHARGE);
    let leaf = finalized(ObjectRole::InodeLeaf, 4, 300);
    let page = finalized(ObjectRole::DirectoryLeaf, 5, 500);
    let oversize = finalized(ObjectRole::InodeBranch, 6, 4096 - OBJECT_CHARGE + 1);
    for object in [
        &whole, &chunk, &value, &leaf, &page, &leaf, &oversize, &value,
    ] {
        staged.offer(object);
    }
    assert_eq!(staged.len(), 4);
    // Staging places nothing: the cache is as it was until admission, and
    // with no length provider an unremembered length is refused.
    assert_eq!(work(&reader), ClientWork::default());
    assert!(reader.file_length(whole.id()).is_err());
    cache.admit_constructed(staged).unwrap();
    let placed = [&value, &leaf, &page];
    let bytes: usize = placed.iter().map(|object| object.canonical_len()).sum();
    assert_eq!(
        work(&reader),
        ClientWork {
            seeded_objects: 3,
            seeded_bytes: bytes as u64,
            seeded_lengths: 1,
            charged_cache_bytes: bytes + 3 * OBJECT_CHARGE + LENGTH_CHARGE,
            cached_objects: 3,
            file_lengths: 1,
            ..ClientWork::default()
        }
    );
    assert_eq!(
        reader.file_length(whole.id()).unwrap(),
        (100 - WHOLE_FILE_CANONICAL_OVERHEAD) as u64
    );
    // A reader is served the placed objects with no upstream demand, and
    // must fetch what was not placed: its upstream does not have them.
    for object in placed {
        assert_eq!(
            reader.read_canonical(object.id()).unwrap(),
            object.canonical()
        );
    }
    assert!(upstream.demanded().is_empty());
    for object in [&whole, &chunk, &oversize] {
        assert_eq!(
            reader.read_canonical(object.id()),
            Err(ContentError::MissingObject)
        );
    }
    assert_eq!(upstream.demanded().len(), 3);
    let after = work(&reader);
    assert_eq!(
        [
            after.cache_hits,
            after.cache_misses,
            after.authenticated_bytes
        ],
        [3, 3, 0]
    );

    // A staging set that is dropped leaves no trace.
    let mut dropped = cache.constructed().unwrap();
    dropped.offer(&finalized(ObjectRole::Symlink, 7, 200));
    drop(dropped);
    assert_eq!(work(&reader), after);

    // The bound keeps the latest: four objects of 1,256 charged bytes do
    // not fit 4,096, so the first is dropped. One already cached is left
    // as it is and is not counted.
    let pages: Vec<_> = (10..14)
        .map(|tag| finalized(ObjectRole::DirectoryBranch, tag, 1000))
        .collect();
    let mut staged = cache.constructed().unwrap();
    for object in pages.iter().chain([&page]) {
        staged.offer(object);
    }
    assert_eq!(staged.len(), 3);
    cache.admit_constructed(staged).unwrap();
    let now = work(&reader);
    assert_eq!(
        [
            now.seeded_objects,
            now.seeded_bytes,
            now.cached_objects as u64
        ],
        [5, (bytes + 2000) as u64, 5]
    );
    assert_eq!(
        now.charged_cache_bytes,
        bytes + 2000 + 5 * OBJECT_CHARGE + LENGTH_CHARGE
    );
    assert_eq!(
        reader.read_canonical(pages[0].id()),
        Err(ContentError::MissingObject)
    );
    assert_eq!(
        reader.read_canonical(pages[3].id()).unwrap(),
        pages[3].canonical()
    );

    // Placed entries are charged to the one allowance and make room like
    // any other: least recently used first, whatever placed them.
    let mut evictions = 0;
    for round in 0..12_u8 {
        let mut staged = cache.constructed().unwrap();
        for tag in 0..3 {
            staged.offer(&finalized(
                ObjectRole::InodeLeaf,
                100 + round * 3 + tag,
                1000,
            ));
        }
        cache.admit_constructed(staged).unwrap();
        let seen = work(&reader);
        assert!(seen.charged_cache_bytes <= allowance);
        evictions = seen.evictions;
    }
    let end = work(&reader);
    assert!(evictions > 0);
    assert_eq!(end.seeded_objects, 5 + 36);
    assert_eq!([end.seeded_lengths, end.file_lengths as u64], [1, 0]);
    assert_eq!(
        end.cached_objects as u64,
        end.seeded_objects + 1 - evictions
    );
    // The oldest placed objects are gone; the latest is held.
    assert_eq!(
        reader.read_canonical(value.id()),
        Err(ContentError::MissingObject)
    );
    assert_eq!(
        reader
            .read_canonical(finalized(ObjectRole::InodeLeaf, 100 + 35, 1000).id())
            .unwrap()
            .len(),
        1000
    );
}

#[test]
fn host_timing_of_a_hit_is_printed_and_not_asserted() {
    for length in [128_usize, 1_024, 8_192, 65_536] {
        let upstream = Arc::new(Upstream::default());
        let entries = if length > 8_192 { 512 } else { 8_192 };
        let client = client(&upstream, entries * (length + OBJECT_CHARGE));
        let ids: Vec<ObjectId> = (0..entries)
            .map(|n| {
                let mut bytes = vec![0_u8; length];
                bytes[..8].copy_from_slice(&(n as u64).to_le_bytes());
                let id = ObjectId::for_bytes(&bytes);
                upstream.objects.lock().unwrap().insert(id, bytes);
                id
            })
            .collect();
        for id in &ids {
            client.read_canonical(*id).unwrap();
        }
        let rounds = if length > 8_192 { 40 } else { 12 };
        let before = work(&client);
        let start = std::time::Instant::now();
        let mut bytes = 0_usize;
        for round in 0..rounds {
            for n in 0..entries {
                bytes += client
                    .read_canonical(ids[(n * 7 + round) % entries])
                    .unwrap()
                    .len();
            }
        }
        let elapsed = start.elapsed();
        let hits = work(&client).cache_hits - before.cache_hits;
        assert_eq!(hits, (rounds * entries) as u64);
        assert_eq!(bytes, rounds * entries * length);
        println!(
            "HOST_HIT_TIMING object_bytes={length} entries={entries} hits={hits} ns_per_hit={}",
            elapsed.as_nanos() / u128::from(hits)
        );
    }
}
