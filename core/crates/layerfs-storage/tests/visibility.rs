//! Publication visibility: what an ordinary reader may see of an unfinished save.
//!
//! The contract under test is that early bounded commits of a save do not make its
//! output available to an unrelated reader, that same-save reads stay unrestricted,
//! and that the boundary is the *publication watermark* rather than the highest
//! pack that happens to be committed.

mod support;

use layerfs_content::inode_leaf::{
    encode_inode_value, InodeKind, InodeLeaf, InodeLeafRow, InodeValue, INODE_VALUE_BYTES,
    LEAF_ROW_BYTES,
};
use layerfs_content::{FinalizedObject, ObjectId, ObjectRole};
use layerfs_storage::encoding::codec::DecompressionWorkspace;
use layerfs_storage::encoding::pool::{PoolIndex, PoolReader};
use layerfs_storage::sqlite::pool::ValueGroupRow;
use layerfs_storage::{SaveHandoff, StorageError, StoragePolicy, Store};
use support::{construct_file, create_store, disabled, noise, open_store, save_all, TempDir};

/// One pooled value, distinct per `seed`.
fn pooled_value(seed: u64) -> [u8; INODE_VALUE_BYTES] {
    let mut bytes = [0_u8; 32];
    bytes[..8].copy_from_slice(&seed.to_be_bytes());
    encode_inode_value(InodeValue {
        kind: InodeKind::RegularFile,
        namespace_ref_count: 1,
        content_root: ObjectId::for_bytes(&bytes),
        metadata_root: ObjectId::for_bytes(&[seed as u8; 8]),
    })
}

/// A canonical pooled inode leaf carrying `values`.
fn pooled_leaf(first_serial: u64, values: &[[u8; INODE_VALUE_BYTES]]) -> FinalizedObject {
    let rows = values
        .iter()
        .enumerate()
        .map(|(index, value)| InodeLeafRow {
            serial: first_serial + index as u64,
            value: *value,
        })
        .collect::<Vec<_>>();
    let canonical = InodeLeaf {
        // The recorded total is the encoded row width: an 8-byte serial plus the
        // 73-byte value, exactly as the reference encoder writes it.
        subtree_bytes: rows.len() as u64 * LEAF_ROW_BYTES as u64,
        rows,
    }
    .encode()
    .expect("canonical pooled leaf");
    FinalizedObject::new(ObjectRole::InodeLeaf, canonical).expect("finalized pooled leaf")
}

/// The first catalogue row whose value-group pack is above `ceiling`.
fn value_group_above(path: &std::path::Path, ceiling: i64) -> Option<ValueGroupRow> {
    let connection = rusqlite::Connection::open(path).unwrap();
    connection
        .query_row(
            "SELECT first_ordinal, count, pack_id, group_number, digest \
             FROM metadata_value_groups WHERE pack_id > ?1 ORDER BY first_ordinal LIMIT 1",
            [ceiling],
            |row| {
                Ok(ValueGroupRow {
                    first_ordinal: row.get(0)?,
                    count: row.get::<_, i64>(1)? as usize,
                    pack_id: row.get(2)?,
                    group_number: row.get::<_, i64>(3)? as usize,
                    digest: ObjectId::from_bytes(&row.get::<_, Vec<u8>>(4)?).unwrap(),
                })
            },
        )
        .ok()
}

/// Highest pack id present, and the published watermark, read out of band.
fn pack_state(path: &std::path::Path) -> (i64, i64) {
    let connection = rusqlite::Connection::open(path).unwrap();
    let highest: i64 = connection
        .query_row(
            "SELECT COALESCE(MAX(pack_id), 0) FROM object_packs",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let ceiling: i64 = connection
        .query_row(
            "SELECT retained_pack_ceiling FROM store_policy WHERE id = 1",
            [],
            |row| row.get(0),
        )
        .unwrap();
    (ceiling, highest)
}

/// One object id whose row lives in a pack above `ceiling`.
fn object_above(path: &std::path::Path, ceiling: i64) -> Option<ObjectId> {
    let connection = rusqlite::Connection::open(path).unwrap();
    let bytes: Option<Vec<u8>> = connection
        .query_row(
            "SELECT object_id FROM objects WHERE pack_id > ?1 LIMIT 1",
            [ceiling],
            |row| row.get(0),
        )
        .ok();
    bytes.map(|bytes| ObjectId::from_bytes(&bytes).unwrap())
}

#[test]
fn an_unrelated_reader_cannot_see_an_open_save_but_sees_it_after_acknowledgement() {
    let dir = TempDir::new("visibility_open");
    let path = dir.store_path("visibility_open");
    let store = create_store(&path);

    // Large enough to force early bounded commits during acceptance.
    let body = noise(5 * 1024 * 1024);
    let (collected, root, _) = construct_file(&body);
    let ids: Vec<ObjectId> = collected
        .objects()
        .iter()
        .map(|(id, _, _, _)| *id)
        .collect();

    let mut operation = disabled(|scope| store.begin_save(scope.child("storage.begin"))).unwrap();
    for (_, role, bytes, references) in collected.objects() {
        let object = layerfs_content::FinalizedObject::new(*role, bytes.clone())
            .unwrap()
            .with_references(references.clone());
        disabled(|_scope| operation.accept(object)).unwrap();
    }

    let (ceiling, highest) = pack_state(&path);
    assert!(
        highest > ceiling,
        "the save must have committed packs early: ceiling={ceiling} highest={highest}"
    );

    // An object that is committed but not yet published must be refused, and
    // refused for the right reason: the pack is beyond the watermark.
    let unpublished = object_above(&path, ceiling).expect("a committed but unpublished object");
    let error = disabled(|scope| store.read_batch(&[unpublished], scope.child("storage.read")))
        .unwrap_err();
    assert!(
        matches!(error, StorageError::VisibilityCeiling { .. }),
        "expected a visibility refusal, got {error}"
    );

    // The whole object set is likewise unavailable.
    let error = disabled(|scope| store.read_batch(&ids, scope.child("storage.read"))).unwrap_err();
    assert!(
        matches!(
            error,
            StorageError::VisibilityCeiling { .. } | StorageError::ObjectMissing(_)
        ),
        "got {error}"
    );

    let outcome = disabled(|scope| operation.finish(scope.child("storage.finish"))).unwrap();
    assert!(outcome.inserted > 0);

    let (ceiling_after, highest_after) = pack_state(&path);
    assert_eq!(
        ceiling_after, highest_after,
        "acknowledgement publishes every pack the save created"
    );
    let (values, _) =
        disabled(|scope| store.read_batch(&[root, unpublished], scope.child("read"))).unwrap();
    assert_eq!(values.len(), 2);
}

#[test]
fn a_definite_failure_never_publishes_and_never_moves_the_watermark() {
    let dir = TempDir::new("visibility_failure");
    let path = dir.store_path("visibility_failure");
    let store = create_store(&path);

    let retained = noise(600_000);
    let (retained_objects, retained_root, _) = construct_file(&retained);
    save_all(&store, &retained_objects).unwrap();
    let ceiling_before = pack_state(&path).0;
    assert!(ceiling_before > 0);

    // A streamed file large enough to commit bounded transactions early, whose
    // source then fails. Construction feeds the save directly, so the failure
    // arrives after this save has already published packs.
    let body = noise(4 * 1024 * 1024);
    let failing = support::FailingAfter::new(body, 4 * 1024 * 1024 - 4_096);
    let policy = layerfs_content::ConstructionPolicy::frozen_default();
    let error = disabled(|scope| {
        let mut operation = store.begin_save(scope.child("storage.begin"))?;
        let mut handoff = SaveHandoff::new(&mut operation);
        let constructed = layerfs_content::construct_stream(
            policy,
            &policy.capacities(),
            failing,
            &mut handoff,
            scope.child("content"),
        );
        if let Some(failure) = handoff.take_failure() {
            return Err(failure);
        }
        constructed.map_err(StorageError::Content)?;
        operation.finish(scope.child("storage.finish"))?;
        Ok(())
    })
    .unwrap_err();
    assert!(
        matches!(error, StorageError::Content(_)),
        "the input failure is reported once: {error}"
    );

    let (ceiling_after, highest_after) = pack_state(&path);
    assert_eq!(
        ceiling_after, ceiling_before,
        "a failure must not advance the watermark, even though it committed packs early"
    );
    assert_eq!(
        highest_after, ceiling_before,
        "cleanup removed the failed attempt's packs"
    );

    let (values, _) =
        disabled(|scope| store.read_batch(&[retained_root], scope.child("read"))).unwrap();
    let expected = retained_objects
        .objects()
        .iter()
        .find(|entry| entry.0 == retained_root)
        .expect("the retained root is one of the accepted objects")
        .2
        .clone();
    assert_eq!(values[0], expected);
}

#[test]
fn an_uninspected_store_refuses_to_start_a_new_save() {
    let dir = TempDir::new("visibility_uninspected");
    let path = dir.store_path("visibility_uninspected");
    {
        let store = create_store(&path);
        let (objects, _, _) = construct_file(&noise(600_000));
        save_all(&store, &objects).unwrap();
    }
    let (ceiling, highest) = pack_state(&path);
    assert_eq!(ceiling, highest);

    // Simulate a save whose cleanup did not complete: a pack and an object above
    // the watermark remain, exactly as an interrupted cleanup would leave them.
    {
        let connection = rusqlite::Connection::open(&path).unwrap();
        connection
            .execute(
                "INSERT INTO object_packs (pack_id, data) VALUES (?1, ?2)",
                rusqlite::params![highest + 1, vec![0u8; 32]],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO objects \
                 (object_id, object_role, canonical_length, pack_id, group_number, record_number) \
                 VALUES (?1, 1, 100, ?2, 0, 0)",
                rusqlite::params![vec![0x7eu8; 32], highest + 1],
            )
            .unwrap();
    }

    let store = open_store(&path);
    let attempt = disabled(|scope| store.begin_save(scope.child("storage.begin")));
    match attempt {
        Err(StorageError::UninspectedState {
            ceiling: reported,
            highest_pack_id,
        }) => {
            assert_eq!(reported, ceiling);
            assert_eq!(highest_pack_id, highest + 1);
        }
        Err(other) => panic!("expected an uninspected-state refusal, got {other}"),
        Ok(_) => panic!("an uninspected Store must not accept a new save"),
    }
}

#[test]
fn a_watermark_ahead_of_storage_is_rejected_at_open() {
    let dir = TempDir::new("visibility_ahead");
    let path = dir.store_path("visibility_ahead");
    {
        let store = create_store(&path);
        let (objects, _, _) = construct_file(&noise(600_000));
        save_all(&store, &objects).unwrap();
    }
    {
        let connection = rusqlite::Connection::open(&path).unwrap();
        connection
            .execute(
                "UPDATE store_policy SET retained_pack_ceiling = retained_pack_ceiling + 5 WHERE id = 1",
                [],
            )
            .unwrap();
    }
    match disabled(|scope| Store::open(&path, scope.child("store"))) {
        Err(StorageError::Integrity(_)) => {}
        Err(other) => panic!("expected an integrity refusal, got {other}"),
        Ok(_) => panic!("a watermark ahead of storage must be rejected at open"),
    }
}

#[test]
fn the_watermark_survives_reopen_and_still_hides_a_later_open_save() {
    let dir = TempDir::new("visibility_reopen");
    let path = dir.store_path("visibility_reopen");
    let first = noise(600_000);
    let (first_objects, first_root, _) = construct_file(&first);
    {
        let store = create_store(&path);
        save_all(&store, &first_objects).unwrap();
    }
    let (ceiling, highest) = pack_state(&path);
    assert_eq!(ceiling, highest);

    let reopened = open_store(&path);
    let (values, counters) =
        disabled(|scope| reopened.read_batch(&[first_root], scope.child("read"))).unwrap();
    assert_eq!(values.len(), 1);
    assert_eq!(
        counters.ceiling, ceiling,
        "a reopened Store reads under its persisted watermark"
    );

    // A later save still hides its own output until it is acknowledged.
    let (second_objects, second_root, _) = construct_file(&noise(5 * 1024 * 1024));
    let mut operation =
        disabled(|scope| reopened.begin_save(scope.child("storage.begin"))).unwrap();
    for (_, role, bytes, references) in second_objects.objects() {
        let object = layerfs_content::FinalizedObject::new(*role, bytes.clone())
            .unwrap()
            .with_references(references.clone());
        disabled(|_scope| operation.accept(object)).unwrap();
    }
    let (mid_ceiling, mid_highest) = pack_state(&path);
    assert!(mid_highest > mid_ceiling);
    let unpublished = object_above(&path, mid_ceiling).expect("an unpublished object");
    let error =
        disabled(|scope| reopened.read_batch(&[unpublished], scope.child("read"))).unwrap_err();
    assert!(
        matches!(error, StorageError::VisibilityCeiling { .. }),
        "got {error}"
    );

    disabled(|scope| operation.finish(scope.child("storage.finish"))).unwrap();
    let values = disabled(|scope| reopened.read_batch(&[second_root], scope.child("read")))
        .unwrap()
        .0;
    assert_eq!(values.len(), 1);
    assert_eq!(StoragePolicy::frozen_default().format_profile(), 1);

    let _ = SaveHandoff::new;
}

#[test]
fn a_pooled_read_refuses_a_value_group_above_the_captured_ceiling() {
    let dir = TempDir::new("visibility_pool");
    let path = dir.store_path("visibility_pool");
    let store = create_store(&path);

    // One pooled leaf, then a body large enough to force the save's bounded early
    // commits. The leaf's value-group pack is then committed while the publication
    // watermark still names the previous completed save.
    let values: Vec<[u8; INODE_VALUE_BYTES]> = (0..8).map(pooled_value).collect();
    let pooled = pooled_leaf(1, &values);
    let body = noise(5 * 1024 * 1024);
    let (collected, _, _) = construct_file(&body);

    let mut operation = disabled(|scope| store.begin_save(scope.child("storage.begin"))).unwrap();
    disabled(|_scope| operation.accept(pooled)).unwrap();
    for (_, role, bytes, references) in collected.objects() {
        let object = FinalizedObject::new(*role, bytes.clone())
            .unwrap()
            .with_references(references.clone());
        disabled(|_scope| operation.accept(object)).unwrap();
    }

    let (ceiling, highest) = pack_state(&path);
    assert!(
        highest > ceiling,
        "the save must have committed packs early: ceiling={ceiling} highest={highest}"
    );
    let row = value_group_above(&path, ceiling).expect("a catalogue row above the watermark");
    let capacities = store.capacities();
    let connection = rusqlite::Connection::open(&path).unwrap();
    let mut workspace = DecompressionWorkspace::new().expect("decode workspace");

    // The group read itself: refused at the watermark, served at the real ceiling.
    let mut reader = PoolReader::new();
    let error = reader
        .group_values(&connection, &capacities, ceiling, &mut workspace, &row)
        .unwrap_err();
    assert!(
        matches!(error, StorageError::VisibilityCeiling { .. }),
        "expected a visibility refusal, got {error}"
    );
    let served = reader
        .group_values(&connection, &capacities, highest, &mut workspace, &row)
        .expect("the published ceiling serves the same group");
    assert_eq!(served.len(), row.count);
    // The group is now retained by this reader's decoded-value cache. The ceiling
    // is decided before the cache is consulted, so the refusal does not depend on
    // the cache being cold.
    let error = reader
        .group_values(&connection, &capacities, ceiling, &mut workspace, &row)
        .unwrap_err();
    assert!(
        matches!(error, StorageError::VisibilityCeiling { .. }),
        "a cached group must not bypass the ceiling, got {error}"
    );

    // The index recurrence reads the catalogue, so it must refuse the same row.
    let mut index = PoolIndex::new();
    let error = index
        .sync(
            &connection,
            &capacities,
            ceiling,
            &mut reader,
            &mut workspace,
        )
        .unwrap_err();
    assert!(
        matches!(error, StorageError::VisibilityCeiling { .. }),
        "expected a visibility refusal from the recurrence, got {error}"
    );

    // The candidate probe resolves values through their group and refuses too.
    let mut index = PoolIndex::new();
    index
        .sync(
            &connection,
            &capacities,
            highest,
            &mut reader,
            &mut workspace,
        )
        .expect("the published ceiling synchronizes the index");
    let error = index
        .find(
            &connection,
            &capacities,
            ceiling,
            &mut reader,
            &mut workspace,
            std::slice::from_ref(&served[0]),
        )
        .unwrap_err();
    assert!(
        matches!(error, StorageError::VisibilityCeiling { .. }),
        "expected a visibility refusal from the probe, got {error}"
    );
    let found = index
        .find(
            &connection,
            &capacities,
            highest,
            &mut reader,
            &mut workspace,
            std::slice::from_ref(&served[0]),
        )
        .expect("the published ceiling resolves the value");
    assert_eq!(found.get(&served[0]).copied(), Some(row.first_ordinal));

    // Acknowledgement publishes the save, and the ordinary read path then serves
    // the pooled leaf through the same ceiling.
    let outcome = disabled(|scope| operation.finish(scope.child("storage.finish"))).unwrap();
    assert!(outcome.pool.leaves >= 1);
    let (ceiling_after, highest_after) = pack_state(&path);
    assert_eq!(ceiling_after, highest_after);
    let mut reader = PoolReader::new();
    let served = reader
        .group_values(
            &connection,
            &capacities,
            ceiling_after,
            &mut workspace,
            &row,
        )
        .expect("the group is published with the save");
    assert_eq!(served.len(), row.count);
}

/// An ordinary-lane group above the ceiling is refused **before** the cache.
///
/// The decoded-group cache belongs to the read operation, and the ceiling does
/// not: it is re-read for every wave. So a body decoded under an older, higher
/// ceiling must never answer for a location the wave's current ceiling hides -
/// the check has to come first. This drives a resolver directly with one shared
/// cache: the group is decoded and cached under an unbounded ceiling, then the
/// same location is resolved under a ceiling below its pack and must be refused.
#[test]
fn an_ordinary_group_above_the_ceiling_is_refused_before_the_decoded_cache() {
    use layerfs_storage::encoding::delta::read::{ChainCounters, Resolver};
    use layerfs_storage::encoding::GroupCache;
    use layerfs_storage::sqlite::lookup;

    let dir = TempDir::new("visibility_group_cache");
    let path = dir.store_path("visibility_group_cache");
    let store = create_store(&path);
    let bytes = noise(200_000);
    let (collected, root, _) = construct_file(&bytes);
    save_all(&store, &collected).expect("save");

    // The product's own open path, so the connection carries the declared profile.
    let connection =
        layerfs_storage::sqlite::connection::open(&path, false).expect("profile connection");
    let capacities = store.capacities();
    let mut workspace = DecompressionWorkspace::new().expect("workspace");
    let mut packs = std::collections::BTreeMap::new();
    let mut cache = GroupCache::new();
    let mut counters = ChainCounters::default();
    let location = lookup::location(&connection, root, i64::MAX)
        .expect("location")
        .expect("the saved root has a locator");

    // Under an unbounded ceiling the object resolves, and its group is retained.
    let (canonical, verified) = Resolver::new(
        &connection,
        i64::MAX,
        &capacities,
        &mut packs,
        &mut cache,
        &mut workspace,
        &mut counters,
    )
    .resolve_at(location)
    .expect("the object resolves under an unbounded ceiling");
    assert_eq!(verified, root);
    assert!(
        cache.retained_bytes() > 0,
        "the resolution retained the group body it decoded"
    );

    // The same location, the same cache, a ceiling below its pack: the refusal
    // must come from visibility, not from the cache.
    let hidden = location.pack_id - 1;
    let error = Resolver::new(
        &connection,
        hidden,
        &capacities,
        &mut packs,
        &mut cache,
        &mut workspace,
        &mut counters,
    )
    .resolve_at(location)
    .expect_err("a location above the ceiling is refused");
    assert!(
        matches!(error, StorageError::VisibilityCeiling { pack_id, ceiling }
            if pack_id == location.pack_id && ceiling == hidden),
        "expected a visibility refusal, got {error}"
    );
    assert_eq!(canonical.len(), location.canonical_length);
}

/// The pooled lane supplies the owner's own ceiling at every read site.
///
/// `MutationOwner::acquire` refuses to start whenever the publication watermark
/// lags the highest stored pack, so no state reachable through the public API
/// today hands the pooled lane a catalogue row above its ceiling. This case is
/// therefore the fail-closed half of the invariant: it reads the pooled read
/// sites out of the product source and rejects an unbounded ceiling there, so a
/// later change that relaxes `acquire` cannot silently re-open the hole. The
/// behavioural half - that the pooled read API refuses such a row - is
/// `a_pooled_read_refuses_a_value_group_above_the_captured_ceiling`.
#[test]
fn the_pooled_lane_supplies_the_owners_ceiling_at_every_read_site() {
    let source = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/cas/pool_lane.rs"),
    )
    .expect("owner source");
    let selection = source.find("fn select_pooled(").expect("pooled selection");
    let synchronization = source
        .find("fn sync_pool_index(")
        .expect("index synchronization");
    let outcomes = source
        .find("/// Pooled lane outcomes of this operation.")
        .expect("pooled outcomes");
    for (label, region) in [
        ("selection", &source[selection..synchronization]),
        (
            "synchronization and base acquisition",
            &source[synchronization..outcomes],
        ),
    ] {
        assert!(
            !region.contains("i64::MAX"),
            "the pooled lane's {label} region passes an unbounded read ceiling"
        );
        assert!(
            region.contains("self.ceiling"),
            "the pooled lane's {label} region does not supply the owner's ceiling"
        );
    }
}

/// R40: an interrupted save leaves a readable Store and a named operator path.
///
/// A death between a mid-save `COMMIT` and the watermark transaction leaves packs
/// above `retained_pack_ceiling`. Every later save is refused with
/// `UninspectedState`, by design: writing would have to guess the ownership of
/// packs whose save never published. The contract supplies no recovery service, so
/// what this case pins is the half that must keep working - **reads** - and the two
/// explicit operator choices, both taken outside the product, that end the refusal.
#[test]
fn an_interrupted_save_leaves_the_store_readable_until_an_operator_decides() {
    let dir = TempDir::new("visibility_interrupted");
    let path = dir.store_path("visibility_interrupted");
    let (objects, root, _) = construct_file(&noise(600_000));
    {
        let store = create_store(&path);
        save_all(&store, &objects).unwrap();
    }
    let (ceiling, highest) = pack_state(&path);
    let interrupted = highest + 1;
    {
        let connection = rusqlite::Connection::open(&path).unwrap();
        connection
            .execute(
                "INSERT INTO object_packs (pack_id, data) VALUES (?1, ?2)",
                rusqlite::params![interrupted, vec![0u8; 32]],
            )
            .unwrap();
    }

    // The Store reopens, the published tree still reads, and the watermark is
    // unchanged: the interrupted save published nothing.
    let reopened = open_store(&path);
    let (values, counters) =
        disabled(|scope| reopened.read_batch(&[root], scope.child("read"))).unwrap();
    assert_eq!(values.len(), 1);
    assert_eq!(
        counters.ceiling, ceiling,
        "reads still run under the watermark"
    );
    assert_eq!(pack_state(&path), (ceiling, interrupted));
    // A save is refused, and the refusal names both numbers the operator needs.
    match disabled(|scope| reopened.begin_save(scope.child("storage.begin"))) {
        Err(StorageError::UninspectedState {
            ceiling: reported,
            highest_pack_id,
        }) => {
            assert_eq!(reported, ceiling);
            assert_eq!(highest_pack_id, interrupted);
        }
        Err(other) => panic!("expected an uninspected-state refusal, got {other}"),
        Ok(_) => panic!("an uninspected Store must not accept a new save"),
    }
    drop(reopened);

    // Operator choice (a): discard the unpublished packs. The two statements below
    // are the whole path - they delete only rows above the watermark, which is what
    // the product's own cleanup does - and the next save is accepted.
    {
        let connection = rusqlite::Connection::open(&path).unwrap();
        connection
            .execute(
                "DELETE FROM objects WHERE pack_id > (SELECT retained_pack_ceiling FROM store_policy WHERE id = 1)",
                [],
            )
            .unwrap();
        connection
            .execute(
                "DELETE FROM object_packs WHERE pack_id > (SELECT retained_pack_ceiling FROM store_policy WHERE id = 1)",
                [],
            )
            .unwrap();
    }
    assert_eq!(pack_state(&path), (ceiling, ceiling));
    let recovered = open_store(&path);
    let (again, _) = disabled(|scope| recovered.read_batch(&[root], scope.child("read"))).unwrap();
    assert_eq!(again.len(), 1);
    let mut operation =
        disabled(|scope| recovered.begin_save(scope.child("storage.begin"))).unwrap();
    let (more, _, _) = construct_file(&noise(400_000));
    for object in more.finalized() {
        operation.accept(object).unwrap();
    }
    let outcome = disabled(|scope| operation.finish(scope.child("storage.finish"))).unwrap();
    assert!(outcome.inserted > 0, "the Store accepts saves again");
    assert_eq!(pack_state(&path).0, pack_state(&path).1);
}
