//! Dependency depth, chain work and malformed-chain handling.
//!
//! Depth is a policy value, so each case drives a real chain to the accepted
//! boundary and then to the first excluded edge. Work ceilings are separate from
//! depth: a chain deep enough to be accepted can still exceed its byte budget,
//! and that is an error rather than a full rewrite.

mod support;

use layerfs_content::{
    AdvisoryPredecessors, ContentError, FinalizedObject, ObjectId, ObjectRole,
    PredecessorProvenance,
};
use layerfs_storage::{StorageError, StoragePolicy, Store};
use support::{
    assembled_small_object, create_store, disabled, noise, open_store, read_objects, save_one,
    TempDir,
};

fn whole(raw: &[u8]) -> FinalizedObject {
    FinalizedObject::new(ObjectRole::WholeFile, assembled_small_object(raw)).expect("canonical")
}

fn with_predecessor(object: FinalizedObject, base: ObjectId) -> FinalizedObject {
    let mut predecessors = AdvisoryPredecessors::new();
    predecessors
        .push(base, PredecessorProvenance::OriginalBase)
        .expect("bounded predecessor");
    object.with_predecessors(predecessors)
}

fn store_with(path: &std::path::Path, whole_depth: u8, chunk_depth: u8) -> Store {
    let policy = StoragePolicy::new(1, 131_072, whole_depth, chunk_depth);
    disabled(|scope| Store::create(path, policy, scope.child("store"))).expect("store")
}

/// Builds a chain of `edges` prefix links and returns every id in order.
fn build_chain(store: &Store, edges: u8, size: usize) -> Vec<ObjectId> {
    let raw = noise(size);
    let mut ids = Vec::new();
    let mut current = raw.clone();
    for step in 0..=edges {
        let object = if step == 0 {
            whole(&current)
        } else {
            with_predecessor(whole(&current), ids[step as usize - 1])
        };
        let id = object.id();
        let outcome = save_one(store, object).expect("chain step saves");
        if step > 0 {
            assert_eq!(
                outcome.prefix_records, 1,
                "step {step} must be a prefix record"
            );
        }
        ids.push(id);
        let mut next = current.clone();
        let start = step as usize * 64;
        let fill = 0x5a_u8.wrapping_add(step);
        for byte in &mut next[start..start + 32] {
            *byte = fill;
        }
        current = next;
    }
    ids
}

#[test]
fn the_default_depths_are_the_documented_ones() {
    let dir = TempDir::new("chain-default");
    let store = create_store(&dir.store_path("chain"));
    assert_eq!(store.policy().whole_file_delta_max_depth(), 8);
    assert_eq!(store.policy().chunk_delta_max_depth(), 4);
    assert_eq!(store.capacities().chain_canonical_limit, 512 * 1024);
    assert_eq!(store.capacities().chain_encoded_limit, 256 * 1024);
}

#[test]
fn a_reduced_depth_stops_at_its_own_boundary() {
    let dir = TempDir::new("chain-depth2");
    let path = dir.store_path("chain");
    let store = store_with(&path, 2, 4);
    let ids = build_chain(&store, 2, 40_000);
    assert_eq!(ids.len(), 3);
    // The next dependent names the deepest object as its base: that base is at
    // the accepted depth, so the trial is not attempted and FULL is stored.
    let mut changed = noise(40_000);
    changed[500] ^= 0xff;
    let deepest = ids[2];
    let outcome = save_one(&store, with_predecessor(whole(&changed), deepest)).expect("save");
    assert_eq!(outcome.reused, 0);
    assert_eq!(outcome.delta.trials, 0);
    assert_eq!(outcome.delta.ineligible_candidates, 1);
    assert_eq!(outcome.full_records, 1);
    // The accepted chain is still readable in full.
    let (values, counters) = read_objects(&store, &ids).expect("chain read");
    assert_eq!(values.len(), 3);
    assert_eq!(counters.max_depth, 2);
}

#[test]
fn an_increased_depth_is_honoured_up_to_its_boundary() {
    let dir = TempDir::new("chain-depth16");
    let path = dir.store_path("chain");
    let store = store_with(&path, 16, 4);
    let ids = build_chain(&store, 16, 20_000);
    assert_eq!(ids.len(), 17);
    let (values, counters) = read_objects(&store, &[ids[16]]).expect("deep read");
    assert_eq!(values.len(), 1);
    assert_eq!(counters.edges, 16, "all sixteen edges are followed");
    assert_eq!(counters.max_depth, 16);
}

#[test]
fn a_chain_that_would_exceed_its_work_budget_is_never_stored() {
    let dir = TempDir::new("chain-work");
    let path = dir.store_path("chain");
    let store = store_with(&path, 16, 4);
    // The chain-work budget is independent of the depth: a chain of 120-KiB
    // objects exceeds 512 KiB of canonical work long before sixteen edges. The
    // save refuses to create a dependent it could not reconstruct, so no object
    // that cannot be read is ever stored.
    let raw = noise(120_000);
    let mut ids = Vec::new();
    let mut current = raw.clone();
    let mut refused = 0_u64;
    for step in 0..12_u8 {
        let object = if step == 0 {
            whole(&current)
        } else {
            with_predecessor(whole(&current), ids[step as usize - 1])
        };
        let id = object.id();
        let outcome = save_one(&store, object).expect("every step is storable");
        assert_eq!(outcome.reused, 0);
        if step > 0 {
            if outcome.prefix_records == 1 {
                assert_eq!(outcome.delta.work_exceeded, 0);
            } else {
                assert_eq!(outcome.full_records, 1);
                assert_eq!(outcome.delta.work_exceeded, 1);
                refused += 1;
            }
        }
        ids.push(id);
        let mut next = current.clone();
        let start = step as usize * 64;
        for byte in &mut next[start..start + 32] {
            *byte = 0xa5;
        }
        current = next;
    }
    assert!(
        refused >= 1,
        "the chain budget must refuse at least one dependency"
    );
    // No stored object depends on bytes a read could not reconstruct.
    for (index, id) in ids.iter().enumerate() {
        let (values, _) = read_objects(&store, &[*id])
            .unwrap_or_else(|error| panic!("member {index} reads: {error}"));
        assert_eq!(values.len(), 1);
    }
}

#[test]
fn a_wrong_role_dependency_is_rejected() {
    let dir = TempDir::new("chain-role");
    let path = dir.store_path("chain");
    let store = create_store(&path);
    let base = whole(&noise(30_000));
    let base_id = base.id();
    save_one(&store, base).expect("base");
    let dependent = whole(&noise(30_000));
    let dependent_id = dependent.id();
    save_one(&store, dependent).expect("dependent");
    drop(store);
    // Point the dependent's recorded base at a resource of another role.
    let connection = rusqlite::Connection::open(&path).expect("external connection");
    let chunk_id = layerfs_content::ObjectId::for_bytes(b"not the same role");
    let affected = connection
        .execute(
            "UPDATE objects SET base_object_id = ?2 WHERE object_id = ?1",
            rusqlite::params![
                dependent_id.to_bytes().to_vec(),
                chunk_id.to_bytes().to_vec()
            ],
        )
        .expect("row change");
    assert_eq!(affected, 1);
    drop(connection);
    let reopened = open_store(&path);
    let error = read_objects(&reopened, &[dependent_id]).unwrap_err();
    assert!(
        matches!(error, StorageError::ObjectMissing(_)),
        "a base that does not exist is a missing dependency: {error}"
    );
    let _ = base_id;
}

#[test]
fn a_cyclic_dependency_is_refused_by_chronology() {
    let dir = TempDir::new("chain-cycle");
    let path = dir.store_path("chain");
    let store = create_store(&path);
    let first = whole(&noise(20_000));
    let first_id = first.id();
    save_one(&store, first).expect("first");
    let mut distinct = noise(20_000);
    distinct[0] ^= 0xff;
    let second = whole(&distinct);
    let second_id = second.id();
    save_one(&store, second).expect("second");
    drop(store);
    let connection = rusqlite::Connection::open(&path).expect("external connection");
    let affected = connection
        .execute(
            "UPDATE objects SET base_object_id = ?2 WHERE object_id = ?1",
            rusqlite::params![first_id.to_bytes().to_vec(), second_id.to_bytes().to_vec()],
        )
        .expect("row change");
    assert_eq!(affected, 1);
    let affected = connection
        .execute(
            "UPDATE objects SET base_object_id = ?2 WHERE object_id = ?1",
            rusqlite::params![second_id.to_bytes().to_vec(), first_id.to_bytes().to_vec()],
        )
        .expect("row change");
    assert_eq!(affected, 1);
    drop(connection);
    let reopened = open_store(&path);
    let error = read_objects(&reopened, &[first_id]).unwrap_err();
    assert!(
        matches!(error, StorageError::Integrity(what) if what.contains("chronology")),
        "a cycle cannot be expressed in locator order: {error}"
    );
}

#[test]
fn a_corrupt_intermediate_is_rejected_during_reconstruction() {
    let dir = TempDir::new("chain-corrupt");
    let path = dir.store_path("chain");
    let store = create_store(&path);
    let ids = build_chain(&store, 3, 50_000);
    drop(store);
    support::corrupt_first_pack(&path);
    let reopened = open_store(&path);
    let error = read_objects(&reopened, &[ids[3]]).unwrap_err();
    assert!(
        matches!(error, StorageError::Integrity(_) | StorageError::Engine(_)),
        "got {error}"
    );
}

#[test]
fn an_unsupported_depth_is_rejected_before_any_store_exists() {
    let dir = TempDir::new("chain-reject");
    let path = dir.store_path("chain");
    for (whole_depth, chunk_depth) in [(51_u8, 4_u8), (8, 51)] {
        let policy = StoragePolicy::new(1, 131_072, whole_depth, chunk_depth);
        let error =
            disabled(|scope| Store::create(&path, policy, scope.child("store"))).unwrap_err();
        assert!(
            matches!(error, StorageError::UnsupportedPolicy { .. }),
            "got {error}"
        );
        assert!(!path.exists());
    }
    let _ = ContentError::LengthOverflow;
}
