//! Configurable policy, derived capacities and the larger singleton lane.
//!
//! A Store persists one accepted policy and validates it again on open. The
//! cutoff, the depths and the chain/work budgets move independently: a larger
//! cutoff widens what one whole-file record may hold and never widens a chain or
//! live-memory budget, and an unsupported value is rejected before any mutation.

mod support;

use layerfs_content::{
    ConstructionPolicy, FinalizedObject, ObjectRole, MINIMUM_SMALL_FILE_THRESHOLD_BYTES,
};
use layerfs_storage::{StorageError, StoragePolicy, Store};
use support::{
    disabled, noise, open_store, patterned, read_objects, save_one, save_via_handoff, TempDir,
};

fn policy_for(cutoff: u64, whole_depth: u8, chunk_depth: u8) -> StoragePolicy {
    StoragePolicy::new(1, cutoff, whole_depth, chunk_depth)
}

fn create(path: &std::path::Path, policy: StoragePolicy) -> Store {
    disabled(|scope| Store::create(path, policy, scope.child("store"))).expect("store creation")
}

#[test]
fn every_supported_cutoff_is_persisted_and_reopened_unchanged() {
    for cutoff in [131_072_u64, 262_144, 1_048_576] {
        let dir = TempDir::new("policy-cutoff");
        let path = dir.store_path("policy");
        let store = create(&path, policy_for(cutoff, 8, 4));
        assert_eq!(store.policy().small_file_threshold_bytes(), cutoff);
        assert_eq!(
            store.capacities().whole_file_canonical_limit,
            cutoff as usize - 1 + 23
        );
        drop(store);
        let reopened = open_store(&path);
        assert_eq!(reopened.policy().small_file_threshold_bytes(), cutoff);
        assert_eq!(reopened.capacities(), store_capacities(cutoff));
    }
}

fn store_capacities(cutoff: u64) -> layerfs_storage::StorageCapacities {
    StorageCapacities::from_policy(policy_for(cutoff, 8, 4)).expect("capacities")
}

use layerfs_storage::StorageCapacities;

#[test]
fn an_unsupported_or_conflicting_policy_is_rejected_before_mutation() {
    let dir = TempDir::new("policy-reject");
    let path = dir.store_path("policy");
    for policy in [
        policy_for(1_048_577, 8, 4),
        policy_for(65_536, 8, 4),
        policy_for(196_608, 8, 4),
        policy_for(131_072, 51, 4),
        policy_for(131_072, 8, 51),
        StoragePolicy::new(2, 131_072, 8, 4),
    ] {
        let error =
            disabled(|scope| Store::create(&path, policy, scope.child("store"))).unwrap_err();
        assert!(
            matches!(error, StorageError::UnsupportedPolicy { .. }),
            "got {error}"
        );
        assert!(!path.exists(), "nothing was created");
    }

    // A conflicting override on an existing Store is refused, and the stored
    // policy is untouched.
    let store = create(&path, policy_for(131_072, 8, 4));
    drop(store);
    let connection = rusqlite::Connection::open(&path).expect("external connection");
    let affected = connection
        .execute(
            "UPDATE store_policy SET small_file_threshold_bytes = 1048576 WHERE id = 1",
            [],
        )
        .expect("out-of-band policy change");
    assert_eq!(affected, 1);
    drop(connection);
    // The Store opens with the changed policy only when that value is supported;
    // the changed value is supported here, so it must be accepted consistently.
    let reopened = open_store(&path);
    assert_eq!(reopened.policy().small_file_threshold_bytes(), 1_048_576);

    // A persisted policy outside the supported range fails the open.
    drop(reopened);
    let connection = rusqlite::Connection::open(&path).expect("external connection");
    // A value inside the DDL range that the policy itself rejects: the schema
    // range is necessary, not sufficient, and the policy check still runs.
    let affected = connection
        .execute(
            "UPDATE store_policy SET small_file_threshold_bytes = 196608 WHERE id = 1",
            [],
        )
        .expect("out-of-range policy");
    assert_eq!(affected, 1);
    drop(connection);
    let error = disabled(|scope| Store::open(&path, scope.child("store"))).unwrap_err();
    assert!(
        matches!(error, StorageError::UnsupportedPolicy { .. }),
        "got {error}"
    );
}

#[test]
fn a_larger_incompressible_whole_file_record_uses_the_singleton_lane() {
    let dir = TempDir::new("policy-singleton");
    let path = dir.store_path("policy");
    let cutoff = 1_048_576_u64;
    let store = create(&path, policy_for(cutoff, 8, 4));
    let construction = ConstructionPolicy::new(cutoff, 8, 4)
        .validated()
        .expect("supported cutoff");
    let capacities = construction.capacities();
    let raw = noise(capacities.whole_file_raw_limit);
    let canonical =
        layerfs_content::file::encode_whole_file(&capacities, &raw).expect("whole-file object");
    let id = layerfs_content::ObjectId::for_bytes(&canonical);
    let object = FinalizedObject::new(ObjectRole::WholeFile, canonical).expect("canonical");
    let outcome = save_one(&store, object).expect("singleton save");
    assert_eq!(outcome.inserted, 1);
    assert!(
        outcome.packs_created >= 1,
        "the oversized record is placed in its own pack"
    );
    drop(store);

    let reopened = open_store(&path);
    let (values, counters) = read_objects(&reopened, &[id]).expect("singleton read");
    assert_eq!(values.len(), 1);
    assert_eq!(values[0].len(), raw.len() + 23);
    assert_eq!(counters.packs_read, 1);
    // The on-disk Store is one database and no payload staging file.
    let entries: Vec<_> = std::fs::read_dir(dir.path())
        .expect("directory")
        .map(|entry| entry.expect("entry").file_name())
        .collect();
    assert_eq!(entries.len(), 1, "entries: {entries:?}");
}

#[test]
fn a_larger_cutoff_keeps_the_chain_and_live_budgets_unchanged() {
    let small = StorageCapacities::from_policy(policy_for(131_072, 8, 4)).expect("capacities");
    let large = StorageCapacities::from_policy(policy_for(1_048_576, 8, 4)).expect("capacities");
    assert_eq!(small.chain_canonical_limit, large.chain_canonical_limit);
    assert_eq!(small.chain_encoded_limit, large.chain_encoded_limit);
    assert_eq!(small.batch_bytes, large.batch_bytes);
    assert_eq!(small.transaction_bytes, large.transaction_bytes);
    assert_eq!(small.pack_limit, large.pack_limit);
    assert_eq!(small.group_limit, large.group_limit);
    assert!(large.whole_file_canonical_limit > small.whole_file_canonical_limit);
    assert!(large.singleton_pack_limit == small.singleton_pack_limit);
    assert!(large.whole_file_frame_limit > small.whole_file_frame_limit);
}

#[test]
fn boundaries_follow_the_configured_cutoff_not_a_frozen_one() {
    for cutoff in [131_072_u64, 262_144] {
        let dir = TempDir::new("policy-boundary");
        let path = dir.store_path("policy");
        let store = create(&path, policy_for(cutoff, 8, 4));
        let construction = ConstructionPolicy::new(cutoff, 8, 4)
            .validated()
            .expect("supported");
        for (length, chunked) in [(cutoff - 1, false), (cutoff, true), (cutoff + 1, true)] {
            let bytes = patterned(length as usize);
            let (collected, root, _) = construct_file_with(construction, &bytes);
            save_via_handoff(&store, &collected).expect("save");
            let (values, _) = read_objects(&store, &[root]).expect("read");
            assert_eq!(values[0], collected_canonical(&collected, root));
            let canonical = values[0].clone();
            let is_chunked = layerfs_content::whole_file_payload(&canonical)
                .expect("framing")
                .is_none();
            assert_eq!(
                is_chunked, chunked,
                "cutoff {cutoff} length {length} representation"
            );
        }
    }
}

fn construct_file_with(
    policy: ConstructionPolicy,
    bytes: &[u8],
) -> (support::Collected, layerfs_content::ObjectId, u64) {
    let mut collected = support::Collected::new();
    let constructed = disabled(|scope| {
        layerfs_content::construct_bytes(
            policy,
            &policy.capacities(),
            bytes,
            &mut collected,
            scope.child("content"),
        )
    })
    .expect("construction");
    (collected, constructed.root, constructed.logical_len)
}

fn collected_canonical(collected: &support::Collected, root: layerfs_content::ObjectId) -> Vec<u8> {
    collected
        .objects()
        .iter()
        .find(|(id, _, _, _)| *id == root)
        .map(|(_, _, bytes, _)| bytes.clone())
        .expect("root is collected")
}

#[test]
fn the_pooled_metadata_depth_is_persisted_and_checked_separately() {
    let dir = TempDir::new("policy-metadata");
    let path = dir.store_path("policy");
    let store = create(&path, policy_for(131_072, 8, 4).with_metadata_depth(12));
    assert_eq!(store.policy().metadata_delta_max_depth(), 12);
    assert_eq!(store.capacities().metadata_delta_max_depth, 12);
    // The pooled bound never widens or narrows a payload bound.
    assert_eq!(store.capacities().whole_file_delta_max_depth, 8);
    assert_eq!(store.capacities().chunk_delta_max_depth, 4);
    assert_eq!(store.capacities().metadata_chain_canonical_limit, 8 * 8_192);
    drop(store);
    let reopened = open_store(&path);
    assert_eq!(reopened.policy().metadata_delta_max_depth(), 12);

    // A persisted value outside the supported range fails the open, and a
    // creation with one is rejected before any file exists.
    drop(reopened);
    let connection = rusqlite::Connection::open(&path).expect("external connection");
    let affected = connection
        .execute(
            "UPDATE store_policy SET metadata_delta_max_depth = 50 WHERE id = 1",
            [],
        )
        .expect("in-range update");
    assert_eq!(affected, 1);
    drop(connection);
    assert_eq!(
        open_store(&path).policy().metadata_delta_max_depth(),
        50,
        "50 is the documented maximum and is accepted"
    );

    let other = dir.store_path("rejected");
    let policy = policy_for(131_072, 8, 4).with_metadata_depth(51);
    let error = disabled(|scope| Store::create(&other, policy, scope.child("store"))).unwrap_err();
    assert!(
        matches!(error, StorageError::UnsupportedPolicy { field } if field == "metadata_delta_max_depth"),
        "got {error}"
    );
    assert!(!other.exists());
}

#[test]
fn the_minimum_supported_cutoff_is_the_published_minimum() {
    assert_eq!(MINIMUM_SMALL_FILE_THRESHOLD_BYTES, 131_072);
    assert_eq!(
        Store::default_policy().small_file_threshold_bytes(),
        MINIMUM_SMALL_FILE_THRESHOLD_BYTES
    );
}

/// A configured depth is a *limit on the chain an operation may build*, never a
/// work or memory budget: depth 4 and depth 50 derive the same limits.
///
/// The reviewed version of this case compared two cutoffs at identical depths, so
/// nothing compared a shallow configuration with a deep one. This case does, and
/// it also pins the depth each role actually uses, which is what a selection
/// consults.
#[test]
fn a_deeper_configured_depth_never_widens_a_work_or_live_budget() {
    let shallow = StorageCapacities::from_policy(policy_for(131_072, 4, 4)).expect("capacities");
    let deep = StorageCapacities::from_policy(policy_for(131_072, 50, 50)).expect("capacities");

    assert_eq!(shallow.whole_file_delta_max_depth, 4);
    assert_eq!(deep.whole_file_delta_max_depth, 50);
    assert_eq!(shallow.chunk_delta_max_depth, 4);
    assert_eq!(deep.chunk_delta_max_depth, 50);

    // Every chain, batch, transaction, pack and group bound is depth-independent.
    assert_eq!(shallow.chain_canonical_limit, deep.chain_canonical_limit);
    assert_eq!(shallow.chain_encoded_limit, deep.chain_encoded_limit);
    assert_eq!(shallow.batch_objects, deep.batch_objects);
    assert_eq!(shallow.batch_bytes, deep.batch_bytes);
    assert_eq!(shallow.transaction_rows, deep.transaction_rows);
    assert_eq!(shallow.transaction_bytes, deep.transaction_bytes);
    assert_eq!(shallow.pack_limit, deep.pack_limit);
    assert_eq!(shallow.singleton_pack_limit, deep.singleton_pack_limit);
    assert_eq!(shallow.group_limit, deep.group_limit);
    assert_eq!(shallow.metadata_group_limit, deep.metadata_group_limit);
    assert_eq!(
        shallow.metadata_chain_canonical_limit,
        deep.metadata_chain_canonical_limit
    );
    assert_eq!(
        shallow.metadata_chain_encoded_limit,
        deep.metadata_chain_encoded_limit
    );
    assert_eq!(
        shallow.whole_file_canonical_limit,
        deep.whole_file_canonical_limit
    );

    // The depth a role consults is the configured one for that role.
    assert_eq!(shallow.delta_depth_for_role(ObjectRole::WholeFile), 4);
    assert_eq!(deep.delta_depth_for_role(ObjectRole::WholeFile), 50);
    assert_eq!(shallow.delta_depth_for_role(ObjectRole::Chunk), 4);
    assert_eq!(deep.delta_depth_for_role(ObjectRole::Chunk), 50);

    // Depth zero disables the prospective delta for that role and nothing else.
    let disabled = StorageCapacities::from_policy(policy_for(131_072, 0, 50)).expect("capacities");
    assert_eq!(disabled.delta_depth_for_role(ObjectRole::WholeFile), 0);
    assert_eq!(disabled.delta_depth_for_role(ObjectRole::Chunk), 50);
    assert_eq!(disabled.chain_canonical_limit, deep.chain_canonical_limit);
}
