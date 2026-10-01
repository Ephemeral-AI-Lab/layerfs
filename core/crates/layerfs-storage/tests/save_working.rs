//! Real actual-Store scoped Save/index/codec reservation and System ownership.
//! Raw readers/consumers and aggregate/native/physical qualification remain open.

#[path = "support/save_working_allocator.rs"]
mod allocation_observer;
mod support;

use layerfs_storage::cas::{SaveWorkingClass, SAVE_INDEX_PAIR_BYTES, SAVE_WORKING_BYTES};
use layerfs_storage::{StorageError, Store};
use support::{create_store, disabled, TempDir};

#[global_allocator]
static ALLOCATOR: allocation_observer::ObservedSystem = allocation_observer::ObservedSystem;
static ALLOCATION_CAPTURE: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[test]
fn actual_hardlink_aliases_share_capacity_before_save_birth_and_known_drop_refunds() {
    let _serial = ALLOCATION_CAPTURE.lock().unwrap();
    let temp = TempDir::new("save_working_aliases");
    let path = temp.store_path("actual");
    let store = create_store(&path);
    let alias = temp.store_path("alias");
    std::fs::hard_link(&path, &alias).unwrap();
    let opened = disabled(|s| Store::open(&alias, s.child("open"))).unwrap();
    assert!(store.same_authority(&opened));
    assert_eq!(
        store.save_working_status().unwrap().shared_index_bytes,
        2 * SAVE_INDEX_PAIR_BYTES
    );
    let first = disabled(|s| store.begin_save(s.child("first"))).unwrap();
    let second = disabled(|s| opened.begin_save(s.child("second"))).unwrap();
    let occupied = store.save_working_status().unwrap();
    assert_eq!(occupied.limit_bytes, SAVE_WORKING_BYTES);
    assert_eq!(
        occupied.reserved_bytes,
        2 * SAVE_INDEX_PAIR_BYTES + 2 * SaveWorkingClass::Standard.bytes()
    );
    assert_eq!(
        opened.save_working_status().unwrap().reserved_bytes,
        occupied.reserved_bytes
    );
    assert_eq!(occupied.owners.len(), 2);
    assert!(occupied
        .owners
        .iter()
        .all(|owner| owner.save_id.is_some() && !owner.retained));
    let db = rusqlite::Connection::open(&path).unwrap();
    let before: i64 = db
        .query_row("SELECT COUNT(*) FROM saves", [], |row| row.get(0))
        .unwrap();
    assert!(matches!(
        disabled(|s| store.begin_save(s.child("third"))),
        Err(StorageError::OwnershipUnavailable)
    ));
    let refused = store.save_working_status().unwrap();
    assert_eq!(refused.byte_refusals, 1);
    assert_eq!(
        refused.last_refused_bytes,
        Some(SaveWorkingClass::Standard.bytes())
    );
    let after: i64 = db
        .query_row("SELECT COUNT(*) FROM saves", [], |row| row.get(0))
        .unwrap();
    assert_eq!(before, after);
    disabled(|s| first.abort(s.child("abort"))).unwrap();
    disabled(|s| second.abort(s.child("abort"))).unwrap();
    assert_eq!(
        store.save_working_status().unwrap().reserved_bytes,
        2 * SAVE_INDEX_PAIR_BYTES
    );
    drop(opened);
    assert_eq!(
        store.save_working_status().unwrap().reserved_bytes,
        SAVE_INDEX_PAIR_BYTES
    );
}

#[test]
fn configured_four_sql_slots_do_not_bypass_two_standard_working_grants() {
    use allocation_observer::Observation;
    let _serial = ALLOCATION_CAPTURE.lock().unwrap();
    let temp = TempDir::new("save_working_configured_four");
    let path = temp.store_path("four");
    let store = create_store(&path);
    disabled(|s| store.set_max_concurrent_writes(4, s.child("configure"))).unwrap();
    assert_eq!(store.max_concurrent_writes().unwrap(), 4);
    let first = disabled(|s| store.begin_save(s.child("first"))).unwrap();
    let second = disabled(|s| store.begin_save(s.child("second"))).unwrap();
    let capture = Observation::start([0, 0], [0, 0]);
    assert!(matches!(
        disabled(|s| store.begin_save(s.child("third"))),
        Err(StorageError::OwnershipUnavailable)
    ));
    let refused = capture.released();
    assert_eq!(
        refused.allocation_events, 0,
        "byte refusal precedes connection/index/codec allocation"
    );
    let status = store.save_working_status().unwrap();
    assert_eq!(status.byte_refusals, 1);
    assert_eq!(
        status.last_refused_bytes,
        Some(SaveWorkingClass::Standard.bytes())
    );
    let db = rusqlite::Connection::open(&path).unwrap();
    let active: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM saves WHERE active_slot IS NOT NULL",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        active, 2,
        "persisted policy4 retained; actual refusal is the byte grant"
    );
    drop(db);
    disabled(|s| first.abort(s.child("abort"))).unwrap();
    disabled(|s| second.abort(s.child("abort"))).unwrap();
}

#[test]
fn selected_singleton_and_standard_use_exact_captured_classes() {
    let temp = TempDir::new("save_working_classes");
    let store = create_store(&temp.store_path("classes"));
    let singleton = disabled(|s| {
        store.begin_save_with_class(SaveWorkingClass::Singleton, s.child("singleton"))
    })
    .unwrap();
    let standard = disabled(|s| store.begin_save(s.child("standard"))).unwrap();
    let status = store.save_working_status().unwrap();
    assert_eq!(status.reserved_bytes, SAVE_WORKING_BYTES);
    assert_eq!(status.owners[0].class, SaveWorkingClass::Singleton);
    assert_eq!(status.owners[1].class, SaveWorkingClass::Standard);
    disabled(|s| singleton.abort(s.child("abort"))).unwrap();
    disabled(|s| standard.abort(s.child("abort"))).unwrap();
    assert_eq!(
        store.save_working_status().unwrap().reserved_bytes,
        SAVE_INDEX_PAIR_BYTES
    );
}

#[test]
fn real_codec_and_index_copy_requested_allocations_release_before_credit() {
    use allocation_observer::{Observation, Pause};
    use layerfs_storage::encoding::codec::ENCODE_WORKSPACE_BYTES;
    let _serial = ALLOCATION_CAPTURE.lock().unwrap();
    let temp = TempDir::new("save_working_allocator");
    let store = create_store(&temp.store_path("observed"));
    let capsule = store.save_working_status().unwrap().retention_capsule_bytes;
    let capture = Observation::start([ENCODE_WORKSPACE_BYTES, capsule], [0, 0]);
    let save = disabled(|s| store.begin_save(s.child("save"))).unwrap();
    let observed = capture.stop();
    assert!(!observed.overflow);
    assert!(observed.live[0] >= ENCODE_WORKSPACE_BYTES + capsule);
    assert!(observed.live_total() < SaveWorkingClass::Standard.bytes());
    {
        let _pause = Pause::new();
        assert_eq!(
            store.save_working_status().unwrap().reserved_bytes,
            SAVE_INDEX_PAIR_BYTES + SaveWorkingClass::Standard.bytes()
        );
    }
    disabled(|s| save.abort(s.child("abort"))).unwrap();
    let after_save = capture.stop();
    let live_sizes = capture.live_sizes();
    #[cfg(target_os = "macos")]
    {
        let mutex_bytes = std::mem::size_of::<nix::libc::pthread_mutex_t>();
        assert_eq!(
            live_sizes,
            vec![(mutex_bytes, 2)],
            "first-lock lazy published-index mutexes remain Store-owned"
        );
        assert_eq!(after_save.live_total(), 2 * mutex_bytes);
        assert_eq!(after_save.live[0], 0, "Save codec arena/capsule freed");
    }
    #[cfg(not(target_os = "macos"))]
    assert_eq!(
        after_save.live_total(),
        0,
        "nonpthread target has no lazy index mutex boxes"
    );
    assert_eq!(
        store.save_working_status().unwrap().reserved_bytes,
        SAVE_INDEX_PAIR_BYTES,
        "actual continuing published-index owners still funded"
    );
    drop(store);
    let released = capture.released();
    assert_eq!(
        released.live_total(),
        0,
        "real captured native-wrapper/index/codec Rust owners dropped"
    );
    eprintln!("actual C2 acquire/known cleanup System requested: {observed:?}; after-Save={after_save:?}; retained-request-sizes={live_sizes:?}; after-Store={released:?}; this is requested Rust layouts, not native allocator metadata/RSS/raw consumers/global fit");
}

#[test]
fn actual_full_pooled_btree_three_copies_fit_prospective_pair_classes() {
    use allocation_observer::Observation;
    use layerfs_storage::encoding::pool::PoolIndex;
    use layerfs_storage::policy::METADATA_INDEX_VALUES;
    let _serial = ALLOCATION_CAPTURE.lock().unwrap();
    // Each real fingerprint/ordinal enters the genuine published implementation.
    // Independent values and observation are external to production source.
    let values: Vec<_> = (0..METADATA_INDEX_VALUES)
        .map(|n| {
            use layerfs_content::inode_leaf::{encode_inode_value, InodeKind, InodeValue};
            encode_inode_value(InodeValue {
                kind: InodeKind::RegularFile,
                namespace_ref_count: 1,
                content_root: layerfs_content::ObjectId::for_bytes(&(n as u64).to_be_bytes()),
                metadata_root: layerfs_content::ObjectId::for_bytes(
                    &((n as u64) ^ 0x5a5a5a5a).to_be_bytes(),
                ),
            })
        })
        .collect();
    let observation = Observation::start([0, 0], [0, 0]);
    let mut shared = PoolIndex::new();
    for (offset, group) in values.chunks(100).enumerate() {
        shared.note_group((offset * 100 + 1) as u32, group).unwrap();
    }
    assert_eq!(shared.len(), METADATA_INDEX_VALUES);
    let one = observation.snapshot();
    assert!(!one.overflow);
    assert!(one.live_total() <= SAVE_INDEX_PAIR_BYTES);
    // One continuous capture owns shared/private/prospective-publication copies.
    let private = shared.clone();
    let publication = private.clone();
    assert_eq!(private.len(), METADATA_INDEX_VALUES);
    assert_eq!(publication.len(), METADATA_INDEX_VALUES);
    let triple = observation.snapshot();
    assert!(!triple.overflow);
    assert!(triple.live_total() <= 3 * SAVE_INDEX_PAIR_BYTES);
    drop(publication);
    drop(private);
    drop(shared);
    let released = observation.released();
    assert_eq!(released.live_total(), 0);
    eprintln!("finite full131072 genuine PoolIndex requested Btree allocation: {one:?}; triple={triple:?}; nominalreported3MiB is not this allocation; private/publication clone values are genuine and exact; arbitrary allocator/global/native/RSS qualification remains open");
}

#[test]
fn actual_shared_reader_unknown_save_birth_retains_prepared_connection_codec_and_credit() {
    let temp = TempDir::new("save_working_unknown_birth");
    let path = temp.store_path("unknown");
    let store = create_store(&path);
    let reader = rusqlite::Connection::open(&path).unwrap();
    reader.execute_batch("BEGIN").unwrap();
    let before: i64 = reader
        .query_row("SELECT COUNT(*) FROM saves", [], |row| row.get(0))
        .unwrap();
    assert_eq!(before, 0);
    let error = disabled(|s| store.begin_save(s.child("unknown")))
        .err()
        .unwrap();
    assert!(error.is_unknown_outcome());
    let retained = store.save_working_status().unwrap();
    assert_eq!(
        retained.reserved_bytes,
        SAVE_INDEX_PAIR_BYTES + SaveWorkingClass::Standard.bytes()
    );
    assert_eq!(retained.owners.len(), 1);
    assert!(retained.owners[0].retained && retained.owners[0].quarantined);
    assert!(retained.owners[0].save_id.is_none());
    reader.execute_batch("ROLLBACK").unwrap();
    drop(reader);
    assert_eq!(
        store.save_working_status().unwrap().reserved_bytes,
        retained.reserved_bytes
    );
    eprintln!("real birthCOMMIT Unknown retained at {}; actual native connection/prepared indices/codec and bytecredit kept; no query/rollback/retry/refund",path.display());
    std::mem::forget(store);
    std::mem::forget(temp);
}

#[test]
fn real_save_publication_transfers_actual_index_ownership_after_known_commit() {
    use allocation_observer::{Observation, Pause};
    let _serial = ALLOCATION_CAPTURE.lock().unwrap();
    let temp = TempDir::new("save_working_publication");
    let store = create_store(&temp.store_path("publication"));
    let (objects, root, _) = support::construct_file(&support::noise(8192));
    let objects = objects.finalized();
    let capture = Observation::start([0, 0], [0, 0]);
    let mut save = disabled(|s| store.begin_save(s.child("save"))).unwrap();
    for object in objects {
        save.accept(object).unwrap();
    }
    let outcome = disabled(|s| save.finish(s.child("finish"))).unwrap();
    let after = capture.stop();
    assert!(!after.overflow);
    assert!(
        after.live_total() > 0,
        "new published index allocations remain in Store"
    );
    assert!(after.peak_total < SaveWorkingClass::Standard.bytes());
    {
        let _pause = Pause::new();
        assert!(disabled(|s| store.contains(&[root], s.child("contains")))
            .unwrap()
            .contains(&root));
        assert_eq!(
            store.save_working_status().unwrap().reserved_bytes,
            SAVE_INDEX_PAIR_BYTES
        );
        assert_eq!(outcome.profile.diag.finish_pool_clone_skipped, 0);
        assert_eq!(outcome.profile.diag.finish_candidate_clone_skipped, 0);
    }
    drop(store);
    let released = capture.released();
    assert_eq!(
        released.live_total(),
        0,
        "actual new shared index deallocation precedes last index credit"
    );
    eprintln!("real Save/native publication requested allocation peak={after:?}; actual Store-last-owner released={released:?}; ordinary v1 roots preserved; no rawconsumer/native/RSS/global qualification");
}

#[test]
fn real_final_publication_unknown_retains_acquired_owner_and_acknowledged_save_id() {
    let temp = TempDir::new("save_working_unknown_publication");
    let path = temp.store_path("unknown");
    let store = create_store(&path);
    let save = disabled(|s| store.begin_save(s.child("save"))).unwrap();
    let identity = store.save_working_status().unwrap().owners[0].save_id;
    let reader = rusqlite::Connection::open(&path).unwrap();
    reader.execute_batch("BEGIN").unwrap();
    let count: i64 = reader
        .query_row("SELECT COUNT(*) FROM saves", [], |row| row.get(0))
        .unwrap();
    assert_eq!(count, 1);
    let error = disabled(|s| save.finish(s.child("finish"))).unwrap_err();
    assert!(error.is_unknown_outcome());
    let retained = store.save_working_status().unwrap();
    assert_eq!(retained.owners.len(), 1);
    assert_eq!(retained.owners[0].save_id, identity);
    assert!(retained.owners[0].retained && retained.owners[0].quarantined);
    reader.execute_batch("ROLLBACK").unwrap();
    drop(reader);
    assert_eq!(
        store.save_working_status().unwrap().reserved_bytes,
        retained.reserved_bytes
    );
    eprintln!("real finalCOMMIT Unknown at {}; acquired nativeSaveID={identity:?}, actual connection/privateindex/codec credit retained; no retry/query/cleanup/refund",path.display());
    std::mem::forget(store);
    std::mem::forget(temp);
}

#[test]
fn actual_unknown_cleanup_retains_connection_owner_and_original_class() {
    let temp = TempDir::new("save_working_unknown_cleanup");
    let path = temp.store_path("cleanup");
    let store = create_store(&path);
    let save = disabled(|s| store.begin_save(s.child("save"))).unwrap();
    let identity = store.save_working_status().unwrap().owners[0].save_id;
    let reader = rusqlite::Connection::open(&path).unwrap();
    reader.execute_batch("BEGIN").unwrap();
    assert_eq!(
        reader
            .query_row::<i64, _, _>("SELECT COUNT(*) FROM saves", [], |row| row.get(0))
            .unwrap(),
        1
    );
    let error = disabled(|s| save.abort(s.child("abort"))).unwrap_err();
    assert!(error.is_unknown_outcome());
    let retained = store.save_working_status().unwrap();
    assert_eq!(retained.owners[0].save_id, identity);
    assert!(retained.owners[0].retained && retained.owners[0].quarantined);
    assert_eq!(
        retained.reserved_bytes,
        SAVE_INDEX_PAIR_BYTES + SaveWorkingClass::Standard.bytes()
    );
    reader.execute_batch("ROLLBACK").unwrap();
    drop(reader);
    assert_eq!(
        store.save_working_status().unwrap().reserved_bytes,
        retained.reserved_bytes
    );
    eprintln!("real cleanupCOMMIT Unknown at {}; native connection and class retained after consuming abort; no retry/refund",path.display());
    std::mem::forget(store);
    std::mem::forget(temp);
}
