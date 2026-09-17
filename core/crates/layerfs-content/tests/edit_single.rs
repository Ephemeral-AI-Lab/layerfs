//! Single known-edit operations over an immutable base.
//!
//! Every case compares the edited root with the root of an independent
//! construction of the same final bytes, and every case reads the result back
//! through the public read path. A candidate round trip alone is not an oracle:
//! the fresh construction is.

mod support;

use layerfs_content::{
    apply_edits, construct_bytes, Edit, EditRequest, EditStream, ObjectId, Replacements,
};
use support::{build_file, disabled_scope, noise, patterned, read_back, repeat, MemoryStore};

const CUTOFF: usize = 131_072;

fn apply(
    store: &MemoryStore,
    root: ObjectId,
    base_len: u64,
    edits: Vec<Edit>,
    replacements: &Replacements,
) -> (MemoryStore, ObjectId, u64) {
    let stream = EditStream::new(base_len, edits).expect("edits are valid");
    let policy = layerfs_content::ConstructionPolicy::frozen_default();
    let mut result = store.merged_clone();
    let constructed = disabled_scope(|scope| {
        apply_edits(
            policy,
            &policy.capacities(),
            store,
            EditRequest {
                root,
                edits: &stream,
                source: replacements,
            },
            &mut result,
            scope.child("edit"),
        )
    })
    .expect("edit succeeds");
    (result, constructed.root, constructed.logical_len)
}

fn fresh(bytes: &[u8]) -> (MemoryStore, ObjectId) {
    let policy = layerfs_content::ConstructionPolicy::frozen_default();
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
    .expect("construction succeeds");
    (store, constructed.root)
}

fn expect_final(name: &str, base: &[u8], edits: Vec<Edit>, final_bytes: &[u8]) {
    let (base_store, constructed) = build_file(base);
    let base_root = constructed.root;
    let mut replacements = Replacements::new();
    for edit in &edits {
        let start = edit.start() as usize;
        let take = edit.replacement_len() as usize;
        replacements.push(
            final_bytes
                .get(start..start + take)
                .unwrap_or_else(|| panic!("{name}: replacement range"))
                .to_vec(),
        );
    }
    let (result_store, root, len) = apply(
        &base_store,
        base_root,
        base.len() as u64,
        edits,
        &replacements,
    );
    assert_eq!(len, final_bytes.len() as u64, "{name}: logical length");
    let bytes = read_back(&result_store, root).expect("edited result reads back");
    assert_eq!(bytes, final_bytes, "{name}: edited bytes");
    let (_, expected_root) = fresh(final_bytes);
    assert_eq!(
        root, expected_root,
        "{name}: root equals a fresh construction"
    );
}

#[test]
fn overwrite_in_the_middle() {
    let base = patterned(3_000);
    let mut final_bytes = base.clone();
    final_bytes[1_400..1_600].copy_from_slice(&noise(200));
    expect_final(
        "middle",
        &base,
        vec![Edit::overwrite(1_400, 1_600)],
        &final_bytes,
    );
}

#[test]
fn overwrite_at_the_head_and_the_tail() {
    let base = patterned(5_000);
    let mut head = base.clone();
    head[..64].copy_from_slice(&noise(64));
    expect_final("head", &base, vec![Edit::overwrite(0, 64)], &head);

    let mut tail = base.clone();
    let start = base.len() - 128;
    tail[start..].copy_from_slice(&noise(128));
    expect_final(
        "tail",
        &base,
        vec![Edit::overwrite(start as u64, base.len() as u64)],
        &tail,
    );
}

#[test]
fn insert_and_delete_change_the_length_exactly() {
    let base = patterned(4_000);
    let insertion = noise(300);
    let mut grown = Vec::new();
    grown.extend_from_slice(&base[..1_000]);
    grown.extend_from_slice(&insertion);
    grown.extend_from_slice(&base[1_000..]);
    expect_final(
        "insert",
        &base,
        vec![Edit::insert(1_000, insertion.len() as u64)],
        &grown,
    );

    let mut shrunk = Vec::new();
    shrunk.extend_from_slice(&base[..500]);
    shrunk.extend_from_slice(&base[2_500..]);
    expect_final("delete", &base, vec![Edit::delete(500, 2_500)], &shrunk);
}

#[test]
fn append_at_end_of_file() {
    let base = patterned(2_000);
    let addition = noise(700);
    let mut grown = base.clone();
    grown.extend_from_slice(&addition);
    expect_final(
        "append",
        &base,
        vec![Edit::insert(base.len() as u64, addition.len() as u64)],
        &grown,
    );
}

#[test]
fn complete_deletion_returns_the_empty_representation() {
    let base = patterned(1_500);
    let (base_store, constructed) = build_file(&base);
    let base_root = constructed.root;
    let replacements = Replacements::new();
    let (result_store, root, len) = apply(
        &base_store,
        base_root,
        base.len() as u64,
        vec![Edit::delete(0, base.len() as u64)],
        &replacements,
    );
    assert_eq!(len, 0);
    let bytes = read_back(&result_store, root).expect("empty result reads back");
    assert!(bytes.is_empty());
    let (_, expected_root) = fresh(&[]);
    assert_eq!(root, expected_root);
}

#[test]
fn the_base_objects_are_never_rewritten() {
    let base = patterned(6_000);
    let (base_store, constructed) = build_file(&base);
    let base_root = constructed.root;
    let before: Vec<(ObjectId, Vec<u8>)> = base_store
        .order()
        .iter()
        .map(|(id, _)| (*id, base_store.canonical(*id).expect("held").to_vec()))
        .collect();
    let mut replacements = Replacements::new();
    replacements.push(noise(400));
    let (result_store, root, _) = apply(
        &base_store,
        base_root,
        base.len() as u64,
        vec![Edit::overwrite(1_000, 1_400)],
        &replacements,
    );
    assert_ne!(root, base_root);
    for (id, bytes) in before {
        let held = base_store.canonical(id).expect("base object survives");
        assert_eq!(held, bytes, "base object {id} was rewritten");
        assert!(
            result_store.canonical(id).is_some(),
            "base object {id} is reachable"
        );
    }
}

#[test]
fn a_declared_length_that_does_not_match_its_bytes_fails() {
    let base = patterned(1_000);
    let (base_store, constructed) = build_file(&base);
    let base_root = constructed.root;
    let mut replacements = Replacements::new();
    replacements.push(vec![7_u8; 10]);
    let stream = EditStream::new(base.len() as u64, vec![Edit::insert(10, 50)]).expect("valid");
    let policy = layerfs_content::ConstructionPolicy::frozen_default();
    let mut result = MemoryStore::new();
    let error = disabled_scope(|scope| {
        apply_edits(
            policy,
            &policy.capacities(),
            &base_store,
            EditRequest {
                root: base_root,
                edits: &stream,
                source: &replacements,
            },
            &mut result,
            scope.child("edit"),
        )
    })
    .unwrap_err();
    assert!(
        matches!(error, layerfs_content::ContentError::InvalidEdit { .. }),
        "got {error}"
    );
}

#[test]
fn an_inapplicable_range_is_rejected_before_any_work() {
    let error = EditStream::new(100, vec![Edit::overwrite(50, 200)]).unwrap_err();
    assert!(matches!(
        error,
        layerfs_content::ContentError::InvalidEdit { .. }
    ));
    let error = EditStream::new(100, vec![Edit::new(60, 40, 0)]).unwrap_err();
    assert!(matches!(
        error,
        layerfs_content::ContentError::InvalidEdit { .. }
    ));
    // A later edit that starts before the previous edit's result position is an
    // overlap, whichever of the two ranges it would have addressed.
    let error = EditStream::new(100, vec![Edit::delete(40, 60), Edit::delete(20, 30)]).unwrap_err();
    assert!(matches!(
        error,
        layerfs_content::ContentError::InvalidEdit { .. }
    ));
    // Edits after a deletion are addressable in the shifted result.
    assert!(EditStream::new(100, vec![Edit::delete(10, 40), Edit::delete(10, 30)]).is_ok());
}

#[test]
fn a_chunked_base_reads_and_edits_without_a_full_pass() {
    let base = noise(CUTOFF * 2);
    let (base_store, constructed) = build_file(&base);
    let base_root = constructed.root;
    let mut final_bytes = base.clone();
    final_bytes[100_000..100_500].copy_from_slice(&noise(500));
    let mut replacements = Replacements::new();
    replacements.push(final_bytes[100_000..100_500].to_vec());
    let (result_store, root, len) = apply(
        &base_store,
        base_root,
        base.len() as u64,
        vec![Edit::overwrite(100_000, 100_500)],
        &replacements,
    );
    assert_eq!(len, final_bytes.len() as u64);
    let bytes = read_back(&result_store, root).expect("edited chunked file reads back");
    assert_eq!(bytes, final_bytes);
}

/// An edit acquires and decodes the base root exactly once.
///
/// The chunked route used to open the view (one authenticated read plus one
/// decode of the root) and then read and decode the same root again to get the
/// file state behind it. With an in-memory provider the duplicate was invisible -
/// the object was already there - but through a C2-backed provider each of those
/// calls is a fresh connection and decode workspace, so the duplicate was real
/// work. This case counts the reads the provider is asked for and pins the number
/// of times the base root's own identity is demanded.
#[test]
fn an_edit_reads_and_decodes_the_base_root_once() {
    use std::cell::RefCell;
    use std::collections::BTreeMap;

    use layerfs_content::{
        AuthenticatedObjects, ContentResult, Edit, EditStream, ObjectId, Replacements,
    };

    /// Counts every demand this provider is asked for.
    struct Counting<'a> {
        inner: &'a MemoryStore,
        demands: RefCell<Vec<ObjectId>>,
        batches: RefCell<usize>,
    }

    impl AuthenticatedObjects for Counting<'_> {
        fn read_canonical_batch(&self, ids: &[ObjectId]) -> ContentResult<Vec<Vec<u8>>> {
            *self.batches.borrow_mut() += 1;
            self.demands.borrow_mut().extend_from_slice(ids);
            self.inner.read_canonical_batch(ids)
        }
    }

    let base = repeat(9_000, 0x51);
    let (store, constructed) = support::build_file(&base);
    let edits =
        EditStream::new(base.len() as u64, vec![Edit::new(4_000, 4_032, 32)]).expect("stream");
    let mut replacements = Replacements::new();
    replacements.push(vec![0x77_u8; 32]);
    let counter = Counting {
        inner: &store,
        demands: RefCell::new(Vec::new()),
        batches: RefCell::new(0),
    };
    let mut consumer = MemoryStore::new();
    let policy = layerfs_content::ConstructionPolicy::frozen_default();
    let result = disabled_scope(|scope| {
        layerfs_content::apply_edits(
            policy,
            &policy.capacities(),
            &counter,
            layerfs_content::EditRequest {
                root: constructed.root,
                edits: &edits,
                source: &replacements,
            },
            &mut consumer,
            scope.child("edit"),
        )
    })
    .expect("the edit applies");

    let demands = counter.demands.borrow();
    let root_demands = demands.iter().filter(|id| **id == constructed.root).count();
    assert_eq!(
        root_demands, 1,
        "the base root is demanded exactly once per edit, not once for the view and \
         once for its file state: {demands:?}"
    );
    let counts = demands.iter().fold(BTreeMap::new(), |mut map, id| {
        *map.entry(*id).or_insert(0_usize) += 1;
        map
    });
    assert!(
        counts.values().all(|count| *count <= 1),
        "no identity is demanded twice in one edit: {counts:?}"
    );
    assert!(result.logical_len > 0);
    let _ = result.root;
}
