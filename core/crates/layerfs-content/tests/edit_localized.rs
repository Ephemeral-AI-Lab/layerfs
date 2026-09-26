//! Localized edit work: affected paths and boundaries, not the whole mapping.
//!
//! The evidence is external. The provider records every object the operation asks
//! for and the consumer records every object it is handed, so "localized" is a
//! count of demanded identities, not a claim about the implementation. A localized
//! edit must walk its path to the boundary, keep untouched subtrees by identity,
//! and emit only the nodes its final root reaches.
//!
//! Measured anatomy (frozen profile, `noise` content, one overwrite replacing three
//! whole extents starting at an interior extent boundary):
//!
//! | file  | mapping pages | extents | pages read | payloads read | pages emitted |
//! | ----- | ------------- | ------- | ---------- | ------------- | ------------- |
//! | 8 MB  | 5             | 432     | 3          | 3             | 4             |
//! | 24 MB | 12            | 1293    | 4          | 3             | 5             |
//!
//! The mapping grows five-fold while the demanded payload set is identical and the
//! emitted set grows by one page: the read set is the path to the boundary plus the
//! replaced extents, never the retained mapping.

mod support;

use std::cell::RefCell;
use std::collections::BTreeSet;

use layerfs_content::file::mapping::{decode_file_state, decode_node_with_context, ExtentNode};
use layerfs_content::{
    apply_edits, construct_bytes, AuthenticatedObjects, ConstructionPolicy, Edit, EditRequest,
    FinalizedConsumer, FinalizedObject, ObjectId, ObjectRole,
};
use support::{disabled_scope, edits::Edits, edits::Parts, noise, reachable, MemoryStore};

/// Provider that records every demanded object.
struct Recorder<'a> {
    inner: &'a MemoryStore,
    demanded: RefCell<Vec<ObjectId>>,
}

impl AuthenticatedObjects for Recorder<'_> {
    fn read_canonical_batch(
        &self,
        ids: &[ObjectId],
    ) -> layerfs_content::ContentResult<Vec<Vec<u8>>> {
        self.demanded.borrow_mut().extend_from_slice(ids);
        self.inner.read_canonical_batch(ids)
    }
}

/// Consumer that keeps everything an edit emitted, for readback and reachability.
#[derive(Default)]
struct Ledger {
    store: MemoryStore,
    ids: Vec<(ObjectId, ObjectRole)>,
}

impl FinalizedConsumer for Ledger {
    fn accept(&mut self, object: FinalizedObject) -> layerfs_content::ContentResult<()> {
        self.ids.push((object.id(), object.role()));
        self.store.accept(object)
    }
}

impl Ledger {
    /// Objects of `role` among the emissions.
    fn count(&self, role: ObjectRole) -> usize {
        self.ids.iter().filter(|(_, seen)| *seen == role).count()
    }

    /// Identity list of the emitted mapping pages.
    fn mapping_pages(&self) -> Vec<ObjectId> {
        self.ids
            .iter()
            .filter(|(_, role)| matches!(role, ObjectRole::ExtentLeaf | ObjectRole::ExtentBranch))
            .map(|(id, _)| *id)
            .collect()
    }
}

/// Mapping inventory of a stored chunked file.
struct Inventory {
    /// Page identities, the file state's mapping root first.
    pages: Vec<ObjectId>,
    /// Payload identities referenced by the leaves.
    payloads: BTreeSet<ObjectId>,
    /// `(offset, length, payload)` of every extent, in logical order.
    extents: Vec<(u64, u64, ObjectId)>,
}

fn inventory_of(store: &MemoryStore, root: ObjectId) -> Inventory {
    let canonical = store.canonical(root).expect("root bytes");
    assert!(
        layerfs_content::whole_file_payload(canonical)
            .expect("framing")
            .is_none(),
        "this test needs a chunked base"
    );
    let state = decode_file_state(canonical).expect("file state");
    let mut inventory = Inventory {
        pages: Vec::new(),
        payloads: BTreeSet::new(),
        extents: Vec::new(),
    };
    walk(
        store,
        state.mapping_root,
        true,
        0,
        &mut inventory.pages,
        &mut inventory.payloads,
        &mut inventory.extents,
    );
    assert_eq!(
        inventory.extents.iter().map(|(_, len, _)| len).sum::<u64>(),
        state.logical_len,
        "the extent walk must cover the file exactly"
    );
    inventory
}

/// Depth-first walk in logical order: extents with absolute offsets, pages in
/// traversal order with the mapping root first.
fn walk(
    store: &MemoryStore,
    id: ObjectId,
    is_root: bool,
    offset: u64,
    pages: &mut Vec<ObjectId>,
    payloads: &mut BTreeSet<ObjectId>,
    extents: &mut Vec<(u64, u64, ObjectId)>,
) -> u64 {
    let node = decode_node_with_context(store.canonical(id).expect("node bytes"), is_root)
        .expect("canonical page");
    if !pages.contains(&id) {
        pages.push(id);
    }
    match node {
        ExtentNode::Leaf { extents: rows, .. } => {
            let mut cursor = offset;
            for row in rows {
                let payload = row.payload_object_id();
                payloads.insert(payload);
                let len = u64::from(row.logical_length());
                extents.push((cursor, len, payload));
                cursor += len;
            }
            cursor
        }
        ExtentNode::Branch { children, .. } => {
            let mut cursor = offset;
            for child in children {
                cursor = walk(
                    store,
                    child.child_object_id,
                    false,
                    cursor,
                    pages,
                    payloads,
                    extents,
                );
            }
            cursor
        }
    }
}

fn build(bytes: &[u8]) -> (MemoryStore, ObjectId) {
    let policy = ConstructionPolicy::frozen_default();
    let mut store = MemoryStore::new();
    let constructed = disabled_scope(|scope| {
        construct_bytes(
            policy,
            &policy.capacities(),
            bytes,
            &mut store,
            scope.child("content"),
        )
    })
    .expect("construction");
    (store, constructed.root)
}

/// What one edit demanded and emitted.
struct Report {
    demanded: Vec<ObjectId>,
    ledger: Ledger,
    root: ObjectId,
    logical_len: u64,
}

impl Report {
    /// Distinct retained mapping pages demanded.
    fn pages_read(&self, inventory: &Inventory) -> BTreeSet<ObjectId> {
        self.demanded
            .iter()
            .filter(|id| inventory.pages.contains(id))
            .copied()
            .collect()
    }

    /// Retained payloads demanded, with their extent index where the walk saw them.
    fn payloads_read(&self, inventory: &Inventory) -> Vec<(usize, ObjectId)> {
        self.demanded
            .iter()
            .filter(|id| inventory.payloads.contains(id))
            .map(|id| {
                let index = inventory
                    .extents
                    .iter()
                    .position(|(_, _, payload)| payload == id)
                    .expect("a read payload is a walked extent");
                (index, *id)
            })
            .collect()
    }

    /// Emitted pages that are not retained base pages.
    fn new_pages(&self, inventory: &Inventory) -> Vec<ObjectId> {
        self.ledger
            .mapping_pages()
            .into_iter()
            .filter(|id| !inventory.pages.contains(id))
            .collect()
    }
}

/// Applies one edit to `root` reading from `store`, emitting into a fresh ledger.
fn edit(store: &MemoryStore, root: ObjectId, len: u64, edit: Edit, source_len: u64) -> Report {
    let recorder = Recorder {
        inner: store,
        demanded: RefCell::new(Vec::new()),
    };
    let mut ledger = Ledger::default();
    let policy = ConstructionPolicy::frozen_default();
    let mut replacements = Parts::new();
    replacements.push(noise(source_len as usize));
    let stream = Edits::new(len, vec![edit]).expect("valid edit stream");
    let edited = disabled_scope(|scope| {
        apply_edits(
            policy,
            &policy.capacities(),
            &recorder,
            EditRequest {
                root,
                edits: &stream,
                source: &replacements,
            },
            &mut ledger,
            scope.child("edit"),
        )
    })
    .expect("edit succeeds");
    Report {
        demanded: recorder.demanded.into_inner(),
        ledger,
        root: edited.root,
        logical_len: edited.logical_len,
    }
}

/// Store holding the base objects plus everything `report` emitted, so follow-on
/// edits and reachability see one object graph.
fn materialized(base: &MemoryStore, report: &Report) -> MemoryStore {
    let mut store = base.merged_clone();
    store.absorb(&report.ledger.store);
    store
}

/// The three-extent overwrite shape used across file sizes.
fn aligned_shape(inventory: &Inventory) -> (u64, u64) {
    let first = inventory.extents.len() / 4;
    (inventory.extents[first].0, inventory.extents[first + 3].0)
}

#[test]
fn an_extent_aligned_overwrite_reads_only_its_path_and_the_replaced_extents() {
    let base = noise(24 * 1024 * 1024);
    let (store, root) = build(&base);
    let inventory = inventory_of(&store, root);
    assert!(
        inventory.pages.len() >= 8,
        "the fixture must have a multi-page mapping: {} pages",
        inventory.pages.len()
    );
    assert!(
        inventory.extents.len() >= 700,
        "{} extents",
        inventory.extents.len()
    );

    let (start, end) = aligned_shape(&inventory);
    let report = edit(
        &store,
        root,
        base.len() as u64,
        Edit::overwrite(start, end),
        end - start,
    );

    // Pages: the path from the mapping root to the affected leaf, plus the leaves
    // the split/join boundary needs. Never the retained mapping.
    let pages_read = report.pages_read(&inventory);
    assert!(
        pages_read.len() <= 6,
        "read {} of {} mapping pages",
        pages_read.len(),
        inventory.pages.len()
    );
    assert!(
        pages_read.len() * 2 < inventory.pages.len(),
        "a localized edit must not walk the mapping: {} of {} pages",
        pages_read.len(),
        inventory.pages.len()
    );
    // Payloads: only the extents the edit replaces are eligible candidates, so the
    // three deleted extents are the only retained payloads demanded.
    let payloads_read = report.payloads_read(&inventory);
    let first = inventory
        .extents
        .iter()
        .position(|(offset, _, _)| *offset == start)
        .expect("the start is an extent boundary");
    let replaced: Vec<usize> = (first..first + 3).collect();
    assert_eq!(
        payloads_read
            .iter()
            .map(|(index, _)| *index)
            .collect::<Vec<_>>(),
        replaced,
        "only the replaced extents may be read"
    );

    // The only objects demanded are the file state's entry point, retained page
    // identities on the affected path, and the replaced payloads.
    let file_state = store.canonical(root).expect("root bytes");
    let state_object = ObjectId::for_bytes(file_state);
    let recognized: BTreeSet<ObjectId> = inventory
        .pages
        .iter()
        .chain(inventory.payloads.iter())
        .copied()
        .chain(std::iter::once(state_object))
        .collect();
    let distinct: BTreeSet<ObjectId> = report.demanded.iter().copied().collect();
    assert!(
        distinct.is_subset(&recognized),
        "unrecognized objects demanded: {:?}",
        distinct.difference(&recognized).collect::<Vec<_>>()
    );
    assert!(
        distinct.contains(&state_object),
        "the file state must be the entry point of the walk"
    );
    assert_eq!(
        distinct.len(),
        pages_read.len() + payloads_read.len() + 1,
        "the demanded set must be the path, the replaced extents and the file state"
    );

    // Emission: the new payloads and the pages on the affected path only.
    assert!(
        report.new_pages(&inventory).len() <= 6,
        "emitted {} new mapping pages",
        report.new_pages(&inventory).len()
    );
    assert!(
        report.ledger.count(ObjectRole::Chunk) <= 4,
        "emitted {} chunk payloads for a three-extent overwrite",
        report.ledger.count(ObjectRole::Chunk)
    );

    // Retained payloads are reused by identity: every base payload the edit did not
    // replace is still referenced from the new root, and the only new payloads are
    // the replacement's own chunks.
    let merged = materialized(&store, &report);
    let after = inventory_of(&merged, report.root);
    let replaced_payloads: BTreeSet<ObjectId> = replaced
        .iter()
        .map(|index| inventory.extents[*index].2)
        .collect();
    let dropped: BTreeSet<ObjectId> = inventory
        .payloads
        .difference(&after.payloads)
        .copied()
        .collect();
    assert_eq!(
        dropped, replaced_payloads,
        "the only payloads dropped must be the replaced extents"
    );
    let added: BTreeSet<ObjectId> = after
        .payloads
        .difference(&inventory.payloads)
        .copied()
        .collect();
    assert!(
        added.len() <= 4,
        "a three-extent overwrite wrote {} new payloads",
        added.len()
    );

    // Untouched leaves keep their identities, except the leaves the split/join
    // boundary rebalances into new nodes.
    let leaves: Vec<ObjectId> = inventory.pages.iter().skip(1).copied().collect();
    let survivors = leaves
        .iter()
        .filter(|id| reachable(&merged, report.root, **id))
        .count();
    assert!(
        survivors >= leaves.len() - 3,
        "{survivors} of {} leaves survived by identity",
        leaves.len()
    );
    assert_eq!(report.logical_len, base.len() as u64);
    assert_eq!(
        support::read_back(&store, root)
            .expect("base readback")
            .len(),
        base.len()
    );
}

#[test]
fn a_mid_extent_overwrite_reads_only_the_straddling_extents() {
    let base = noise(24 * 1024 * 1024);
    let (store, root) = build(&base);
    let inventory = inventory_of(&store, root);
    let first = inventory.extents.len() / 4;
    // One byte inside each boundary extent: the range cuts two retained payloads.
    let start = inventory.extents[first].0 + 1;
    let end = inventory.extents[first + 2].0 - 1;
    let straddling: BTreeSet<usize> = inventory
        .extents
        .iter()
        .enumerate()
        .filter(|(_, (offset, len, _))| {
            (*offset < start && start < offset + len) || (*offset < end && end < offset + len)
        })
        .map(|(index, _)| index)
        .collect();
    assert_eq!(
        straddling.len(),
        2,
        "the shape must cut exactly two extents"
    );
    let report = edit(
        &store,
        root,
        base.len() as u64,
        Edit::overwrite(start, end),
        end - start,
    );

    let straggling = straddling;
    let read: BTreeSet<usize> = report
        .payloads_read(&inventory)
        .iter()
        .map(|(index, _)| *index)
        .collect();
    assert_eq!(
        read, straggling,
        "a mid-extent overwrite reads exactly the two boundary extents"
    );
    assert!(
        report.pages_read(&inventory).len() <= 6,
        "read {} mapping pages",
        report.pages_read(&inventory).len()
    );
    assert_eq!(report.logical_len, base.len() as u64);
}

#[test]
fn localized_work_is_independent_of_file_size() {
    // Same edit shape, five-fold mapping growth: the demanded set must not follow.
    let mut measurements = Vec::new();
    for megabytes in [8usize, 24] {
        let base = noise(megabytes * 1024 * 1024);
        let (store, root) = build(&base);
        let inventory = inventory_of(&store, root);
        let (start, end) = aligned_shape(&inventory);
        let report = edit(
            &store,
            root,
            base.len() as u64,
            Edit::overwrite(start, end),
            end - start,
        );
        measurements.push((
            megabytes,
            inventory.pages.len(),
            inventory.extents.len(),
            report.pages_read(&inventory).len(),
            report.payloads_read(&inventory).len(),
            report.ledger.count(ObjectRole::ExtentLeaf)
                + report.ledger.count(ObjectRole::ExtentBranch),
        ));
    }
    let (small_mb, small_pages, small_extents, small_read, small_payloads, small_emitted) =
        measurements[0];
    let (large_mb, large_pages, large_extents, large_read, large_payloads, large_emitted) =
        measurements[1];
    assert!(large_pages >= small_pages * 2, "{measurements:?}");
    assert!(large_extents >= small_extents * 2, "{measurements:?}");
    assert_eq!(
        small_payloads, large_payloads,
        "payload reads grew with the file: {measurements:?}"
    );
    assert!(
        large_emitted <= small_emitted + 1,
        "emitted mapping pages grew with the file: {measurements:?}"
    );
    assert!(
        large_emitted < large_pages,
        "{large_mb} MB emitted {large_emitted} of {large_pages} mapping pages"
    );
    assert!(
        large_read <= small_read + 2,
        "page reads grew with the file: {measurements:?}"
    );
    assert!(
        large_read * 2 < large_pages,
        "{large_mb} MB demanded {large_read} of {large_pages} pages"
    );
    assert!(
        small_read < small_pages,
        "{small_mb} MB demanded {small_read} of {small_pages} pages"
    );
}

#[test]
fn an_append_at_end_of_file_touches_only_the_right_boundary() {
    let base = noise(16 * 1024 * 1024);
    let (store, root) = build(&base);
    let inventory = inventory_of(&store, root);
    assert!(
        inventory.pages.len() >= 4,
        "{} pages",
        inventory.pages.len()
    );
    let end = base.len() as u64;
    let report = edit(&store, root, end, Edit::insert(end, 64 * 1024), 64 * 1024);
    let pages_read = report.pages_read(&inventory);
    assert!(
        pages_read.len() <= 3,
        "append read {} mapping pages of {}",
        pages_read.len(),
        inventory.pages.len()
    );
    assert!(
        report.payloads_read(&inventory).is_empty(),
        "an append reads no retained payload"
    );
    let merged = materialized(&store, &report);
    let leaves: Vec<ObjectId> = inventory.pages.iter().skip(1).copied().collect();
    let survivors = leaves
        .iter()
        .filter(|id| reachable(&merged, report.root, **id))
        .count();
    assert!(
        survivors >= leaves.len() - 2,
        "{survivors} of {} leaves survived an append",
        leaves.len()
    );
    assert_eq!(report.logical_len, end + 64 * 1024);
    assert_eq!(
        support::read_back(&merged, report.root)
            .expect("readback")
            .len() as u64,
        end + 64 * 1024
    );
}

#[test]
fn many_localized_edits_keep_each_edit_on_its_own_path() {
    // Six sequential edits against one running result: each edit pays its own path,
    // and no edit's cost grows with the number of edits before it.
    let base = noise(8 * 1024 * 1024);
    let (store, root) = build(&base);
    let inventory = inventory_of(&store, root);
    let mut current = store.merged_clone();
    let mut current_root = root;
    let mut worst_pages = 0usize;
    let mut worst_payloads = 0usize;
    for index in 0..6usize {
        let offset = inventory.extents[inventory.extents.len() / 8 + index].0;
        let report = edit(
            &current,
            current_root,
            base.len() as u64,
            Edit::overwrite(offset, offset + 4096),
            4096,
        );
        worst_pages = worst_pages.max(report.pages_read(&inventory).len());
        worst_payloads = worst_payloads.max(report.payloads_read(&inventory).len());
        current = materialized(&current, &report);
        current_root = report.root;
    }
    assert!(worst_pages <= 4, "worst per-edit page reads {worst_pages}");
    assert!(
        worst_payloads <= 4,
        "worst per-edit payload reads {worst_payloads}"
    );
    let state = decode_file_state(current.canonical(current_root).expect("final root"))
        .expect("final file state");
    assert_eq!(state.logical_len, base.len() as u64);
    assert_eq!(
        support::read_back(&current, current_root)
            .expect("readback")
            .len(),
        base.len()
    );
}

/// Applies one edit and reports its own node-load count, the demanded objects and
/// the resulting root.
fn edit_counters(
    store: &MemoryStore,
    root: ObjectId,
    len: u64,
    edit: Edit,
    source_len: u64,
) -> (u64, usize, ObjectId) {
    let recorder = Recorder {
        inner: store,
        demanded: RefCell::new(Vec::new()),
    };
    let mut ledger = Ledger::default();
    let policy = ConstructionPolicy::frozen_default();
    let mut replacements = Parts::new();
    if source_len > 0 {
        replacements.push(noise(source_len as usize));
    }
    let stream = Edits::new(len, vec![edit]).expect("valid edit stream");
    let edited = disabled_scope(|scope| {
        apply_edits(
            policy,
            &policy.capacities(),
            &recorder,
            EditRequest {
                root,
                edits: &stream,
                source: &replacements,
            },
            &mut ledger,
            scope.child("edit"),
        )
    })
    .expect("edit succeeds");
    (
        edited.counters.nodes_read,
        recorder.demanded.into_inner().len(),
        edited.root,
    )
}

#[test]
fn a_page_two_passes_reach_is_demanded_once_for_the_operation() {
    // P1-7: one `apply_edits` builds one mapping-page memo and hands it to both
    // passes. The comparison reads the replaced range and the split descent walks
    // into the same mapping pages; without the memo each of those pages is a
    // separate provider demand, and with it the second reach is a memo hit. The
    // census is per object identity, so "at most once" is the whole claim: an
    // object demanded twice is a page the operation read twice.
    let bytes = noise(262_144);
    let (store, root) = build(&bytes);
    let inventory = inventory_of(&store, root);
    let (start, end) = aligned_shape(&inventory);
    let recorder = Recorder {
        inner: &store,
        demanded: RefCell::new(Vec::new()),
    };
    let mut ledger = Ledger::default();
    let policy = ConstructionPolicy::frozen_default();
    let removed = end - start;
    let mut replacements = Parts::new();
    replacements.push(noise(removed as usize));
    let stream = Edits::new(bytes.len() as u64, vec![Edit::overwrite(start, end)]).expect("stream");
    let edited = disabled_scope(|scope| {
        apply_edits(
            policy,
            &policy.capacities(),
            &recorder,
            EditRequest {
                root,
                edits: &stream,
                source: &replacements,
            },
            &mut ledger,
            scope.child("edit"),
        )
    })
    .expect("edit succeeds");
    let demanded = recorder.demanded.into_inner();
    let mut counts: std::collections::BTreeMap<ObjectId, usize> = std::collections::BTreeMap::new();
    for id in &demanded {
        *counts.entry(*id).or_default() += 1;
    }
    for (id, count) in &counts {
        assert!(
            *count <= 1,
            "object {id} was demanded {count} times by one operation: {counts:?}"
        );
    }
    assert!(
        counts.len() > 3,
        "the fixture must demand several objects: {counts:?}"
    );
    assert!(
        edited.counters.nodes_read > 0,
        "the operation did read stored nodes"
    );
}

/// The two shapes P1-9 distinguishes: the same extent-aligned range, deleted or
/// overwritten, on one 262,144-byte chunked base.
fn aligned_pair() -> ((u64, usize, ObjectId), (u64, usize, ObjectId)) {
    let bytes = noise(262_144);
    let (store, root) = build(&bytes);
    let inventory = inventory_of(&store, root);
    let (start, end) = aligned_shape(&inventory);
    let removed = end - start;
    (
        edit_counters(
            &store,
            root,
            bytes.len() as u64,
            Edit::delete(start, end),
            0,
        ),
        edit_counters(
            &store,
            root,
            bytes.len() as u64,
            Edit::overwrite(start, end),
            removed,
        ),
    )
}

#[test]
fn a_pure_deletion_never_walks_the_rightmost_path() {
    // P1-9: the predecessor hint is consumed only by the replacement scan, which a
    // pure deletion does not run, so the O(h) rightmost walk must not happen. The
    // walk costs one `load_node` on this shape — and it is served by a node the
    // split already built, so it never reaches the provider: the deletion's
    // provider-demand count is the same with and without the walk, and only
    // `EditCounters::nodes_read` sees it.
    // P1-7 tightened both pins: the comparison pass's mapping pages are
    // memo-served to the construction pass, so the overwrite's totals moved
    // 7/6 -> 6/5 while the deletion's are unchanged (it reads no base range for
    // comparison, because a length-changing edit is a difference by construction).
    // The assertions below state the current counts and the bound each protects.
    let (deletion, overwrite) = aligned_pair();
    assert_eq!(
        deletion.0, 4,
        "the split's own loads, with no rightmost walk"
    );
    assert!(
        deletion.0 <= 4,
        "the split's own loads, with no rightmost walk: {}",
        deletion.0
    );
    assert_eq!(
        deletion.1, 2,
        "and two provider demands: the file state and the mapping path"
    );
    assert!(
        overwrite.0 > deletion.0,
        "the overwrite does walk: {} against {}",
        overwrite.0,
        deletion.0
    );
}

#[test]
fn an_overwrite_still_demands_the_predecessor_path() {
    // The negative pin against over-gating: an overwrite's replacement scan still
    // consumes the rightmost payload hint, so its walk and its scan remain.
    let (deletion, overwrite) = aligned_pair();
    assert_eq!(
        overwrite.0, 6,
        "the split, the rightmost walk and the replacement scan, with the compare \
         pass's mapping page memo-served"
    );
    assert!(
        overwrite.0 <= 7,
        "the split, the rightmost walk and the replacement scan: {}",
        overwrite.0
    );
    assert_eq!(
        overwrite.1, 5,
        "and the payloads the scan demands, which the deletion never touches"
    );
    assert_ne!(
        overwrite.2, deletion.2,
        "two different results from the same base"
    );
}
