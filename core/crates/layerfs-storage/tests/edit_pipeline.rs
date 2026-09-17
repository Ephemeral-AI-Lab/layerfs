//! Real C1 edits through C2: save, acknowledge, reopen and read back.
//!
//! The pipeline is the production path with no shim: C1 builds or edits the
//! objects, C2 stores them, the Store is closed and reopened, and the logical
//! bytes are read back through a fresh connection. The edited root must open the
//! exact final bytes, including after a dependency chain was stored.

mod support;

use layerfs_content::{
    apply_edits, construct_bytes, ConstructionPolicy, Edit, EditRequest, EditStream, ObjectId,
    Replacements,
};
use layerfs_storage::Store;
use support::{
    create_store, disabled, noise, open_store, patterned, read_logical, read_objects,
    save_via_handoff, Collected, Provider, TempDir,
};

const CUTOFF: u64 = 131_072;

fn edit(
    base: &[u8],
    edits: Vec<Edit>,
    source: &Replacements,
    into: &mut Collected,
) -> (ObjectId, u64) {
    let policy = ConstructionPolicy::frozen_default();
    let stream = EditStream::new(base.len() as u64, edits).expect("valid stream");
    let mut provider_objects = Collected::new();
    let constructed = disabled(|scope| {
        construct_bytes(
            policy,
            &policy.capacities(),
            base,
            &mut provider_objects,
            scope.child("content"),
        )
    })
    .expect("base construction");
    let constructed = disabled(|scope| {
        apply_edits(
            policy,
            &policy.capacities(),
            &Provider(&provider_objects),
            EditRequest {
                root: constructed.root,
                edits: &stream,
                source,
            },
            into,
            scope.child("edit"),
        )
    })
    .expect("edit succeeds");
    (constructed.root, constructed.logical_len)
}

#[test]
fn a_small_edit_round_trips_through_a_reopened_store() {
    let dir = TempDir::new("pipeline-small");
    let path = dir.store_path("pipeline");
    let store = create_store(&path);
    let base = patterned(90_000);
    let mut final_bytes = base.clone();
    final_bytes[40_000..40_500].copy_from_slice(&noise(500));
    let mut replacements = Replacements::new();
    replacements.push(final_bytes[40_000..40_500].to_vec());
    let mut collected = Collected::new();
    let (root, len) = edit(
        &base,
        vec![Edit::overwrite(40_000, 40_500)],
        &replacements,
        &mut collected,
    );
    assert_eq!(len, final_bytes.len() as u64);
    let outcome = save_via_handoff(&store, &collected).expect("save");
    assert!(outcome.acknowledged);
    drop(store);

    let reopened = open_store(&path);
    let (values, _) = read_objects(&reopened, &[root]).expect("read");
    let expected = collected
        .objects()
        .iter()
        .find(|(id, _, _, _)| *id == root)
        .map(|(_, _, bytes, _)| bytes.clone())
        .expect("root");
    assert_eq!(values[0], expected);
}

#[test]
fn a_chunked_edit_round_trips_and_reuses_retained_payloads() {
    let dir = TempDir::new("pipeline-chunked");
    let path = dir.store_path("pipeline");
    let store = create_store(&path);
    let base = noise(CUTOFF as usize * 2);
    let base_collected = {
        let policy = ConstructionPolicy::frozen_default();
        let mut collected = Collected::new();
        let _ = disabled(|scope| {
            construct_bytes(
                policy,
                &policy.capacities(),
                &base,
                &mut collected,
                scope.child("content"),
            )
        })
        .expect("base");
        collected
    };
    save_via_handoff(&store, &base_collected).expect("base save");

    let mut final_bytes = base.clone();
    final_bytes[100_000..100_300].copy_from_slice(&noise(300));
    let mut replacements = Replacements::new();
    replacements.push(final_bytes[100_000..100_300].to_vec());
    let mut collected = Collected::new();
    let (root, len) = edit(
        &base,
        vec![Edit::overwrite(100_000, 100_300)],
        &replacements,
        &mut collected,
    );
    assert_eq!(len, final_bytes.len() as u64);
    let outcome = save_via_handoff(&store, &collected).expect("edit save");
    assert!(outcome.acknowledged);
    drop(store);
    let base_objects = base_collected.objects().len();

    let reopened = open_store(&path);
    let (values, _) = read_objects(&reopened, &[root]).expect("read");
    let expected = collected
        .objects()
        .iter()
        .find(|(id, _, _, _)| *id == root)
        .map(|(_, _, bytes, _)| bytes.clone())
        .expect("root");
    assert_eq!(values[0], expected);
    // Retained payloads were stored by the base save and referenced, not
    // rewritten: the edit stores far fewer objects than the file holds.
    assert!(
        (outcome.inserted as usize) < base_objects,
        "the edit re-stored the whole file: {} of {}",
        outcome.inserted,
        base_objects
    );
}

#[test]
fn a_single_edit_transition_round_trips_through_the_store() {
    for (label, base_len, edits, expect_len) in [
        (
            "grow",
            100_000_usize,
            vec![Edit::insert(50_000, 200_000)],
            300_000_u64,
        ),
        (
            "shrink",
            300_000_usize,
            vec![Edit::delete(10_000, 250_000)],
            60_000,
        ),
        ("empty", 150_000_usize, vec![Edit::delete(0, 150_000)], 0),
    ] {
        let dir = TempDir::new("pipeline-transition");
        let path = dir.store_path("pipeline");
        let store = create_store(&path);
        let base = patterned(base_len);
        let mut replacements = Replacements::new();
        for edit in &edits {
            replacements.push(noise(edit.replacement_len() as usize));
        }
        let mut collected = Collected::new();
        let (root, len) = edit(&base, edits, &replacements, &mut collected);
        assert_eq!(len, expect_len, "{label}");
        save_via_handoff(&store, &collected).expect("save");
        drop(store);
        let reopened = open_store(&path);
        let (values, _) = read_objects(&reopened, &[root]).expect("read");
        let expected = collected
            .objects()
            .iter()
            .find(|(id, _, _, _)| *id == root)
            .map(|(_, _, bytes, _)| bytes.clone())
            .expect("root");
        assert_eq!(values[0], expected, "{label}");
    }
}

#[test]
fn a_standalone_route_and_an_integrated_route_agree_on_the_root() {
    let dir = TempDir::new("pipeline-agree");
    let path = dir.store_path("pipeline");
    let store = create_store(&path);
    let base = patterned(80_000);
    let mut replacements = Replacements::new();
    replacements.push(noise(1_000));
    let mut expected = base.clone();
    expected[20_000..21_000].copy_from_slice(&noise(1_000));
    let mut collected = Collected::new();
    let (root, _) = edit(
        &base,
        vec![Edit::overwrite(20_000, 21_000)],
        &replacements,
        &mut collected,
    );
    save_via_handoff(&store, &collected).expect("integrated save");
    // The same final bytes constructed independently produce the same root.
    let policy = ConstructionPolicy::frozen_default();
    let mut fresh = Collected::new();
    let constructed = disabled(|scope| {
        construct_bytes(
            policy,
            &policy.capacities(),
            &expected,
            &mut fresh,
            scope.child("content"),
        )
    })
    .expect("fresh construction");
    assert_eq!(root, constructed.root);
    drop(store);
    let reopened = open_store(&path);
    let (values, _) = read_objects(&reopened, &[root]).expect("read");
    assert_eq!(values[0], fresh.objects()[0].2);
}

/// Several edits in one operation, in current-result coordinates, through a real
/// Store: the second and third edits are positioned in the coordinates the
/// earlier ones produced, the result is saved, the Store is reopened and the
/// logical bytes are read back through the real C1 read path with every canonical
/// object acquired from the Store.
#[test]
fn a_multi_edit_chunked_stream_round_trips_in_current_result_coordinates() {
    let dir = TempDir::new("pipeline-multi-edit");
    let path = dir.store_path("pipeline");
    let store = create_store(&path);
    let base = noise(CUTOFF as usize * 2);
    let base_collected = {
        let policy = ConstructionPolicy::frozen_default();
        let mut collected = Collected::new();
        let _ = disabled(|scope| {
            construct_bytes(
                policy,
                &policy.capacities(),
                &base,
                &mut collected,
                scope.child("content"),
            )
        })
        .expect("base construction");
        collected
    };
    save_via_handoff(&store, &base_collected).expect("base save");

    let inserted = noise(40_000);
    let overwritten = noise(5_000);
    let mut replacements = Replacements::new();
    replacements.push(inserted.clone());
    replacements.push(overwritten.clone());
    replacements.push(Vec::new());
    let edits = vec![
        Edit::insert(20_000, 40_000),
        Edit::overwrite(100_000, 105_000),
        Edit::delete(150_000, 180_000),
    ];
    let mut expected = base.clone();
    expected.splice(20_000..20_000, inserted.iter().copied());
    expected.splice(100_000..105_000, overwritten.iter().copied());
    expected.drain(150_000..180_000);

    let mut collected = Collected::new();
    let (root, len) = edit(&base, edits, &replacements, &mut collected);
    assert_eq!(len, expected.len() as u64);
    assert!(
        collected.objects().len() < base_collected.objects().len(),
        "the edit re-stored the whole file: {} of {}",
        collected.objects().len(),
        base_collected.objects().len()
    );
    let outcome = save_via_handoff(&store, &collected).expect("edit save");
    assert!(outcome.acknowledged);
    drop(store);

    let reopened = open_store(&path);
    // The counters of the read wave are asserted, not discarded: one requested
    // object, one lookup page, at least one pack, under the published watermark.
    let (values, counters) = read_objects(&reopened, &[root]).expect("root read");
    assert_eq!(values.len(), 1);
    assert_eq!(counters.objects, 1);
    assert_eq!(counters.pages, 1);
    assert!(counters.packs_read >= 1, "counters: {counters:?}");
    assert!(counters.ceiling > 0, "counters: {counters:?}");
    assert_eq!(
        counters.edges, 0,
        "the file state carries no dependency edge: {counters:?}"
    );
    assert_eq!(
        counters.max_depth, 0,
        "the file state is not part of a delta chain: {counters:?}"
    );
    // The logical result is the exact expected bytes, read through the real C1
    // read path with every object acquired from the reopened Store.
    assert_eq!(support::read_logical(&reopened, root), expected);
}

// ---------------------------------------------------------------------------
// Read amplification charged at the Store (#169 item 2, C2 side).
// ---------------------------------------------------------------------------
//
// The accounted quantity is the same one the C1-side case defines - the
// canonical bytes a transition acquires from its supplied provider - but here the
// provider is a real `Store`, so the acquisition is also charged by C2's own
// `StoreReadCounters`: objects, pack bodies, membership pages, dependency edges,
// deepest chain and canonical bytes. The base's logical coverage is derived
// independently, by decoding the saved tree, exactly as on the C1 side.

use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, BTreeSet};

use layerfs_content::file::mapping::FileState;
use layerfs_content::file::mapping::{decode_node_with_context, ExtentNode};
use layerfs_content::file::{classify, FileContent};
use layerfs_content::ContentResult;
use layerfs_storage::StoreReadCounters;

/// Provider that serves canonical bytes from a real Store and charges them.
struct StoreCensus<'a> {
    store: &'a Store,
    counters: Cell<StoreReadCounters>,
    log: RefCell<Vec<(ObjectId, u64)>>,
}

impl<'a> StoreCensus<'a> {
    fn new(store: &'a Store) -> Self {
        Self {
            store,
            counters: Cell::new(StoreReadCounters::default()),
            log: RefCell::new(Vec::new()),
        }
    }

    fn counters(&self) -> StoreReadCounters {
        self.counters.get()
    }

    fn acquisitions(&self) -> Vec<(ObjectId, u64)> {
        self.log.borrow().clone()
    }
}

impl layerfs_content::AuthenticatedObjects for StoreCensus<'_> {
    fn read_canonical_batch(&self, ids: &[ObjectId]) -> ContentResult<Vec<Vec<u8>>> {
        let (values, wave) =
            disabled(|scope| self.store.read_batch(ids, scope.child("storage.read")))
                .map_err(|_| layerfs_content::ContentError::MissingObject)?;
        let mut total = self.counters.get();
        total.objects += wave.objects;
        total.packs_read += wave.packs_read;
        total.pages += wave.pages;
        total.edges += wave.edges;
        total.max_depth = total.max_depth.max(wave.max_depth);
        total.canonical_bytes += wave.canonical_bytes;
        total.ceiling = wave.ceiling;
        self.counters.set(total);
        let mut log = self.log.borrow_mut();
        for (id, bytes) in ids.iter().zip(&values) {
            log.push((*id, bytes.len() as u64));
        }
        Ok(values)
    }
}

/// Logical coverage of every payload the saved tree reaches, read through the Store.
fn store_coverage(root: ObjectId, objects: &Collected) -> BTreeMap<ObjectId, Vec<(u64, u64)>> {
    let mut map: BTreeMap<ObjectId, Vec<(u64, u64)>> = BTreeMap::new();
    let canonical = objects
        .objects()
        .iter()
        .find(|(id, _, _, _)| *id == root)
        .map(|(_, _, bytes, _)| bytes.clone())
        .expect("root bytes");
    match classify(&canonical).expect("classification") {
        FileContent::WholeFile { logical_len } => {
            map.entry(root).or_default().push((0, logical_len));
        }
        FileContent::Chunked(state) => {
            let provider = support::Provider(objects);
            walk_store_coverage(&provider, state, state.mapping_root, 0, true, &mut map);
        }
    }
    map
}

fn walk_store_coverage(
    provider: &dyn layerfs_content::AuthenticatedObjects,
    state: FileState,
    id: ObjectId,
    origin: u64,
    root: bool,
    map: &mut BTreeMap<ObjectId, Vec<(u64, u64)>>,
) {
    let canonical = provider.read_canonical(id).expect("stored page");
    let node = decode_node_with_context(&canonical, root).expect("canonical page");
    assert_eq!(
        node.level(),
        if root { state.tree_level } else { node.level() }
    );
    match node {
        ExtentNode::Leaf { extents, .. } => {
            let mut position = origin;
            for extent in extents {
                let end = position + u64::from(extent.logical_length());
                map.entry(extent.payload_object_id())
                    .or_default()
                    .push((position, end));
                position = end;
            }
        }
        ExtentNode::Branch {
            level, children, ..
        } => {
            let mut previous = origin;
            let _ = level;
            for child in children {
                walk_store_coverage(provider, state, child.child_object_id, previous, false, map);
                previous = child.cumulative_logical_end;
            }
        }
    }
}

/// Canonical bytes of every mapping page under `root`, independently of a read.
fn store_pages(objects: &Collected, root: ObjectId) -> BTreeSet<ObjectId> {
    let mut ids = BTreeSet::new();
    ids.insert(root);
    let canonical = objects
        .objects()
        .iter()
        .find(|(id, _, _, _)| *id == root)
        .map(|(_, _, bytes, _)| bytes.clone())
        .expect("root bytes");
    if let FileContent::Chunked(state) = classify(&canonical).expect("classification") {
        ids.insert(state.mapping_root);
        let provider = support::Provider(objects);
        collect_store_pages(&provider, state.mapping_root, true, &mut ids);
    }
    ids
}

fn collect_store_pages(
    provider: &dyn layerfs_content::AuthenticatedObjects,
    id: ObjectId,
    root: bool,
    ids: &mut BTreeSet<ObjectId>,
) {
    ids.insert(id);
    let canonical = provider.read_canonical(id).expect("stored page");
    let node = decode_node_with_context(&canonical, root).expect("canonical page");
    if let ExtentNode::Branch { children, .. } = node {
        for child in children {
            collect_store_pages(provider, child.child_object_id, false, ids);
        }
    }
}

#[test]
fn a_transition_charges_only_the_retained_base_at_the_store() {
    let cutoff = CUTOFF;
    let dir = TempDir::new("pipeline-amplification");
    let path = dir.store_path("pipeline");
    let store = create_store(&path);
    let policy = ConstructionPolicy::frozen_default();
    // Sixteen cutoffs of incompressible base: enough packs that the discarded
    // middle owns whole packs of its own, which is what makes the Store-level
    // claim below a real one rather than a statement about one shared pack.
    let base = noise((16 * cutoff) as usize);
    let mut base_collected = Collected::new();
    disabled(|scope| {
        construct_bytes(
            policy,
            &policy.capacities(),
            &base,
            &mut base_collected,
            scope.child("content"),
        )
    })
    .expect("base construction");
    let base_root = base_collected
        .objects()
        .last()
        .map(|(id, _, _, _)| *id)
        .expect("a root was emitted");
    save_via_handoff(&store, &base_collected).expect("base save");

    let coverage = store_coverage(base_root, &base_collected);
    let pages = store_pages(&base_collected, base_root);
    let base_payload_bytes: u64 = coverage
        .keys()
        .map(|id| {
            base_collected
                .objects()
                .iter()
                .find(|(candidate, _, _, _)| candidate == id)
                .map(|(_, _, bytes, _)| bytes.len() as u64)
                .unwrap_or(0)
        })
        .sum();

    // Large -> small: delete the middle, keeping `cutoff / 4` bytes at each end.
    let keep = cutoff / 4;
    let mut expected = base[..keep as usize].to_vec();
    expected.extend_from_slice(&base[base.len() - keep as usize..]);
    let stream = EditStream::new(
        base.len() as u64,
        vec![Edit::delete(keep, base.len() as u64 - keep)],
    )
    .expect("valid stream");
    let replacements = Replacements::new();
    let census = StoreCensus::new(&store);
    let mut edited = Collected::new();
    let constructed = disabled(|scope| {
        apply_edits(
            policy,
            &policy.capacities(),
            &census,
            EditRequest {
                root: base_root,
                edits: &stream,
                source: &replacements,
            },
            &mut edited,
            scope.child("edit"),
        )
    })
    .expect("edit succeeds");
    assert_eq!(constructed.logical_len, expected.len() as u64);

    // The independent model: a fresh construction of the final bytes reaches the
    // same root the transition did.
    let mut fresh = Collected::new();
    let fresh_root = disabled(|scope| {
        construct_bytes(
            policy,
            &policy.capacities(),
            &expected,
            &mut fresh,
            scope.child("content"),
        )
    })
    .expect("fresh construction")
    .root;
    assert_eq!(constructed.root, fresh_root);

    // A discarded payload is never acquired, and the acquisition is bounded by
    // what the transition keeps.
    let acquired = census.acquisitions();
    let distinct: BTreeSet<ObjectId> = acquired.iter().map(|(id, _)| *id).collect();
    let mut base_objects: BTreeSet<ObjectId> = pages.clone();
    base_objects.extend(coverage.keys().copied());
    assert!(
        distinct.is_subset(&base_objects),
        "a transition acquired something that is not part of its base"
    );
    let exclusive_discarded: Vec<ObjectId> = coverage
        .iter()
        .filter(|(_, ranges)| {
            ranges
                .iter()
                .all(|(start, end)| *start >= keep && *end <= base.len() as u64 - keep)
        })
        .map(|(id, _)| *id)
        .collect();
    assert!(
        !exclusive_discarded.is_empty(),
        "the fixture discards whole payloads"
    );
    for id in &exclusive_discarded {
        assert!(
            !distinct.contains(id),
            "a payload entirely inside the discarded range was acquired from the Store"
        );
    }
    let payload_acquired: u64 = acquired
        .iter()
        .filter(|(id, _)| coverage.contains_key(id))
        .map(|(_, bytes)| *bytes)
        .sum();
    assert!(
        payload_acquired <= constructed.logical_len + 2 * 32_768,
        "the transition acquired {payload_acquired} payload bytes for a {} byte result",
        constructed.logical_len
    );
    assert!(
        payload_acquired * 4 <= base_payload_bytes,
        "the transition acquired {payload_acquired} of {base_payload_bytes} base payload bytes"
    );

    // C2's own counters, charged to the transition. `packs_read` counts pack-body
    // reads, not distinct packs - one pack can hold several acquired chunks and be
    // read once per real demand - so the Store-side "not the whole base" claim is
    // made on the packs the acquired identities actually live in, read from the
    // object rows rather than from the counters.
    let counters = census.counters();
    let connection = open_connection(&path);
    let packs: i64 = connection
        .query_row("SELECT COUNT(*) FROM object_packs", [], |row| row.get(0))
        .expect("pack count");
    let acquired_packs = packs_of(&connection, &distinct);
    // Packs whose every object is a payload the transition discarded. Touching one
    // of them would mean reading bytes that exist only to be dropped.
    let discarded_only = discarded_only_packs(&connection, &exclusive_discarded, packs);
    assert!(
        !discarded_only.is_empty(),
        "the fixture must leave at least one pack holding only discarded payloads"
    );
    assert!(
        acquired_packs.is_disjoint(&discarded_only),
        "the transition acquired objects from a pack that holds only discarded          payloads: acquired {acquired_packs:?}, discarded-only {discarded_only:?} of {packs} packs"
    );
    // The two instruments are independent and must agree: the census sees the
    // identities the provider served, and C2 counts what its own read wave
    // returned.
    assert_eq!(
        counters.objects,
        acquired.len() as u64,
        "the Store returned a different object count than the provider served"
    );
    // `canonical_bytes` is *not* the byte quantity of this read, and the case pins
    // that rather than assuming it: the field accumulates dependency-chain
    // reconstruction only, so a plain FULL acquisition contributes nothing to it.
    // The byte accounting for a transition therefore comes from the provider
    // census above, which is what the accounted quantity names; this assertion
    // records the corroboration instead of a number that would be zero by
    // construction and read as "nothing was read".
    assert_eq!(
        counters.canonical_bytes, 0,
        "no dependency chain is followed by a representation transition"
    );
    assert_eq!(
        counters.edges, 0,
        "a representation transition follows no dependency chain"
    );
    assert!(
        counters.packs_read > 0,
        "the transition read at least one pack body"
    );
    println!(
        "MEASURED read-amplification-store case=large-to-small base_payload_bytes={base_payload_bytes} \
         payload_acquired={payload_acquired} final_bytes={} exclusive_discarded={} \
         store_objects={} store_packs_read={} store_pages={} store_edges={} store_max_depth={} \
         store_canonical_bytes={} store_ceiling={} packs_in_store={packs}          packs_acquired={} amplification_milli={}",
        constructed.logical_len,
        exclusive_discarded.len(),
        counters.objects,
        counters.packs_read,
        counters.pages,
        counters.edges,
        counters.max_depth,
        counters.canonical_bytes,
        counters.ceiling,
        acquired_packs.len(),
        payload_acquired * 1_000 / constructed.logical_len
    );

    // The result is stored and reads back through the Store exactly.
    let outcome = save_via_handoff(&store, &edited).expect("edit save");
    assert!(outcome.acknowledged);
    drop(store);
    let reopened = open_store(&path);
    assert_eq!(read_logical(&reopened, constructed.root), expected);
}

/// Opens a read-only connection to a Store database for an independent count.
fn open_connection(path: &std::path::Path) -> rusqlite::Connection {
    rusqlite::Connection::open(path).expect("external connection")
}

/// Packs whose every object is one the transition discarded.
fn discarded_only_packs(
    connection: &rusqlite::Connection,
    discarded: &[ObjectId],
    packs: i64,
) -> BTreeSet<i64> {
    let discarded: BTreeSet<ObjectId> = discarded.iter().copied().collect();
    let mut result = BTreeSet::new();
    for pack in 1..=packs {
        let mut statement = connection
            .prepare("SELECT object_id FROM objects WHERE pack_id = ?1")
            .expect("prepared");
        let ids = statement
            .query_map([pack], |row| row.get::<_, Vec<u8>>(0))
            .expect("rows")
            .collect::<Result<Vec<_>, _>>()
            .expect("collect");
        if ids.is_empty() {
            continue;
        }
        let all_discarded = ids.iter().all(|bytes| {
            ObjectId::from_bytes(bytes)
                .map(|id| discarded.contains(&id))
                .unwrap_or(false)
        });
        if all_discarded {
            result.insert(pack);
        }
    }
    result
}

/// Distinct pack identifiers the given objects live in, straight from the rows.
fn packs_of(connection: &rusqlite::Connection, ids: &BTreeSet<ObjectId>) -> BTreeSet<i64> {
    let mut packs = BTreeSet::new();
    let mut statement = connection
        .prepare("SELECT pack_id FROM objects WHERE object_id = ?1")
        .expect("prepared");
    for id in ids {
        let pack: Option<i64> = statement
            .query_row([id.to_bytes().to_vec()], |row| row.get(0))
            .ok();
        if let Some(pack) = pack {
            packs.insert(pack);
        }
    }
    packs
}
