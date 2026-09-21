//! Exact reuse, repeated identities and collision rejection.

mod support;

use layerfs_content::{FinalizedObject, ObjectId, ObjectRole};
use layerfs_storage::{StorageError, Store};
use support::{
    construct_file, create_store, disabled, noise, open_store, patterned, read_objects, repeat,
    save_all, TempDir,
};

fn tamper_pack(path: &std::path::Path) {
    let connection = rusqlite::Connection::open(path).expect("external connection");
    let pack_id: i64 = connection
        .query_row("SELECT MIN(pack_id) FROM object_packs", [], |row| {
            row.get(0)
        })
        .expect("a stored pack");
    let mut data = support::read_pack_row(&connection, pack_id);
    let last = data.len() - 1;
    data[last] ^= 0x01;
    support::write_pack_row(&connection, pack_id, &data);
}

#[test]
fn resaving_the_same_object_reuses_its_row_without_a_new_pack() {
    let dir = TempDir::new("reuse");
    let path = dir.store_path("reuse");
    let bytes = patterned(5_000);
    let (collected, root, _) = construct_file(&bytes);
    let store = create_store(&path);

    let first = save_all(&store, &collected).expect("first save");
    assert_eq!(first.inserted, 1);
    assert_eq!(first.packs_created, 1);

    let second = save_all(&store, &collected).expect("second save");
    assert_eq!(second.inserted, 0, "nothing new was written");
    assert_eq!(
        second.reused, 1,
        "the occurrence was served by the existing row"
    );
    assert_eq!(second.packs_created, 0);
    assert_eq!(
        second.commits, 2,
        "slot acquisition and publication, no payload writes"
    );

    let (values, counters) = read_objects(&store, &[root]).unwrap();
    assert_eq!(counters.packs_read, 1);
    assert_eq!(values[0], collected.objects()[0].2);
}

#[test]
fn repeated_identities_inside_one_batch_insert_once_and_reuse_the_rest() {
    let dir = TempDir::new("repeat");
    let path = dir.store_path("repeat");
    // Identical chunks make one payload identity appear many times in one batch.
    let mut bytes = Vec::new();
    for _ in 0..40 {
        bytes.extend_from_slice(&repeat(8_192, 0x77));
    }
    let (collected, root, _) = construct_file(&bytes);
    let distinct: std::collections::BTreeSet<_> = collected
        .objects()
        .iter()
        .map(|(id, _, _, _)| *id)
        .collect();
    assert!(
        distinct.len() < collected.objects().len(),
        "the tree references the same chunk more than once"
    );

    let store = create_store(&path);
    let outcome = save_all(&store, &collected).expect("save succeeds");
    assert_eq!(outcome.inserted, distinct.len() as u64);
    assert_eq!(
        outcome.reused,
        (collected.objects().len() - distinct.len()) as u64
    );

    let (values, _) = read_objects(&store, &[root]).unwrap();
    assert_eq!(values.len(), 1);
}

#[test]
fn a_repetitive_file_spanning_waves_inserts_once_per_distinct_identity() {
    let dir = TempDir::new("repeat_waves");
    let path = dir.store_path("repeat_waves");
    // One repeated byte. Every chunk resolves to the same identity, and 2 MiB of it
    // is far larger than the 512-object / 512 KiB preparation batch, so the repeated
    // identity is offered again after its first occurrence has already been prepared
    // but while its group is still open. A duplicate row here is a hard constraint
    // failure, and a store that fails on repeated content fails on the workload it
    // exists for.
    let bytes = repeat(2 * 1024 * 1024, 0x21);
    let (collected, root, _) = construct_file(&bytes);
    let distinct: std::collections::BTreeSet<_> = collected
        .objects()
        .iter()
        .map(|(id, _, _, _)| *id)
        .collect();
    assert!(
        collected.objects().len() > 3 * distinct.len(),
        "the workload must repeat identities many times: {} objects, {} distinct",
        collected.objects().len(),
        distinct.len()
    );

    let store = create_store(&path);
    let outcome = save_all(&store, &collected).expect("a repetitive file is an ordinary workload");
    assert_eq!(
        outcome.inserted,
        distinct.len() as u64,
        "one row per distinct identity"
    );
    assert_eq!(
        outcome.reused,
        (collected.objects().len() - distinct.len()) as u64
    );

    // Every stored identity reads back as the canonical bytes construction emitted.
    let expected: std::collections::BTreeMap<_, _> = collected
        .objects()
        .iter()
        .map(|(id, _, bytes, _)| (*id, bytes.clone()))
        .collect();
    let ids: Vec<_> = expected.keys().copied().collect();
    let (values, _) = read_objects(&store, &ids).unwrap();
    for (id, value) in ids.iter().zip(values) {
        assert_eq!(&value, expected.get(id).unwrap());
    }
    assert_eq!(
        read_objects(&store, &[root]).unwrap().0.len(),
        1,
        "the file root reads back"
    );
}

#[test]
fn repeated_identity_across_batches_reuses_without_rewriting() {
    let dir = TempDir::new("across");
    let path = dir.store_path("across");
    let bytes = noise(131_072);
    let (collected, root, _) = construct_file(&bytes);
    let store = create_store(&path);
    let first = save_all(&store, &collected).unwrap();
    assert!(first.inserted >= 2);

    let chunk = collected
        .objects()
        .iter()
        .find(|(_, role, _, _)| *role == ObjectRole::Chunk)
        .map(|(id, _, _, _)| *id)
        .expect("a chunk");
    let outcome = disabled(|scope| {
        let mut operation = store.begin_save(scope.child("storage.begin"))?;
        operation.accept(
            collected
                .finalized()
                .into_iter()
                .find(|object| object.id() == chunk)
                .unwrap(),
        )?;
        operation.finish(scope.child("storage.finish"))
    })
    .expect("second save");
    assert_eq!(outcome.inserted, 0);
    assert_eq!(outcome.reused, 1);
    assert_eq!(outcome.commits, 2, "slot acquisition and publication");
    let (values, _) = read_objects(&store, &[root, chunk]).unwrap();
    assert_eq!(values.len(), 2);
}

#[test]
fn a_corrupted_stored_record_is_refused_before_its_identity_is_trusted() {
    let dir = TempDir::new("collision");
    let path = dir.store_path("collision");
    let bytes = patterned(6_000);
    let (collected, _, _) = construct_file(&bytes);
    let store = create_store(&path);
    save_all(&store, &collected).expect("first save");
    drop(store);
    tamper_pack(&path);

    let reopened = open_store(&path);
    let error = save_all(&reopened, &collected).unwrap_err();
    // Exact variant, not a union. The reviewed version accepted
    // `Collision | Integrity` and named the case after the collision branch, but
    // a tampered record never reaches an identity comparison: the record is a
    // framed Zstandard payload and its own checksum refuses it first. Asserting
    // that exact refusal keeps the case honest about which check it exercises.
    assert!(
        matches!(error, StorageError::Integrity("Zstandard codec failure")),
        "a tampered record produced {error}"
    );
}

#[test]
fn a_stored_row_with_a_short_length_is_rejected_without_reading_it() {
    let dir = TempDir::new("short");
    let path = dir.store_path("short");
    let bytes = patterned(6_000);
    let (collected, root, _) = construct_file(&bytes);
    let store = create_store(&path);
    save_all(&store, &collected).expect("first save");
    drop(store);

    let connection = rusqlite::Connection::open(&path).unwrap();
    let affected = connection
        .execute(
            "UPDATE objects SET canonical_length = canonical_length - 1 WHERE object_id = ?1",
            rusqlite::params![root.to_bytes().to_vec()],
        )
        .unwrap();
    assert_eq!(affected, 1);
    drop(connection);

    let reopened = open_store(&path);
    let error = save_all(&reopened, &collected).unwrap_err();
    assert!(matches!(error, StorageError::Collision(_)), "got {error}");
}

#[test]
fn one_operation_can_save_two_files_into_shared_packs() {
    let dir = TempDir::new("shared");
    let path = dir.store_path("shared");
    let first = repeat(60_000, 0x11);
    let second = repeat(60_000, 0x22);
    let (first_objects, first_root, _) = construct_file(&first);
    let (second_objects, second_root, _) = construct_file(&second);
    assert_eq!(first_objects.objects().len(), 1);
    assert_eq!(second_objects.objects().len(), 1);

    let store = create_store(&path);
    let outcome = disabled(|scope| {
        let mut operation = store.begin_save(scope.child("storage.begin"))?;
        for object in first_objects
            .finalized()
            .into_iter()
            .chain(second_objects.finalized())
        {
            operation.accept(object)?;
        }
        operation.finish(scope.child("storage.finish"))
    })
    .expect("save succeeds");
    assert_eq!(outcome.inserted, 2);
    assert_eq!(outcome.packs_created, 1, "both objects share one pack");

    let (values, counters) = read_objects(&store, &[first_root, second_root]).unwrap();
    assert_eq!(counters.packs_read, 1);
    assert_eq!(values[0], first_objects.objects()[0].2);
    assert_eq!(values[1], second_objects.objects()[0].2);
}

#[test]
fn a_directory_is_never_treated_as_a_store() {
    let dir = TempDir::new("directory");
    let error = disabled(|scope| Store::open(dir.path(), scope.child("store"))).unwrap_err();
    assert!(matches!(error, StorageError::Engine(_)), "got {error}");
}

/// R33: a batch lookup is bounded by the declared read ceiling, not by the slice.
///
/// `Store::contains` took the caller's whole slice and paged it into queries, so
/// its size was the caller's choice rather than a declared resource. It now
/// refuses a demand above `READ_OBJECT_LIMIT` before a connection is opened - the
/// same ceiling `read_batch` already applies - and this case pins both sides of
/// the boundary plus the read-side behaviour below it.
#[test]
fn a_batch_lookup_is_bounded_by_the_declared_read_ceiling() {
    use layerfs_storage::policy::READ_OBJECT_LIMIT;

    let dir = TempDir::new("contains-bound");
    let path = dir.store_path("contains-bound");
    let store = create_store(&path);
    let (objects, root, _) = construct_file(&noise(200_000));
    save_all(&store, &objects).expect("save");

    let demand = |count: usize, first: ObjectId| {
        (0..count)
            .map(|index| {
                let mut bytes = first.to_bytes();
                bytes[0] ^= (index & 0xff) as u8;
                bytes[1] ^= (index >> 8) as u8;
                ObjectId::from_bytes(&bytes).expect("identity")
            })
            .collect::<Vec<_>>()
    };

    // At the ceiling the query runs.
    let at_limit = demand(READ_OBJECT_LIMIT, root);
    let found = disabled(|scope| store.contains(&at_limit, scope.child("contains")))
        .expect("a demand at the declared ceiling is accepted");
    assert!(
        found.len() <= 1,
        "the synthetic identifiers are not stored: {found:?}"
    );
    // The real identity is reported present when it is in the demand.
    let present = disabled(|scope| store.contains(&[root], scope.child("contains")))
        .expect("one identifier is accepted");
    assert_eq!(present, vec![root]);

    // One past the ceiling is refused, before any connection is opened.
    let over = disabled(|scope| {
        store.contains(
            &demand(READ_OBJECT_LIMIT + 1, root),
            scope.child("contains"),
        )
    })
    .unwrap_err();
    match over {
        StorageError::CapacityExceeded {
            what,
            limit,
            actual,
        } => {
            assert_eq!(what, "storage.read_objects");
            assert_eq!(limit, READ_OBJECT_LIMIT as u64);
            assert_eq!(actual, READ_OBJECT_LIMIT as u64 + 1);
        }
        other => panic!("expected a capacity refusal, got {other}"),
    }
}

/// The read wave does not hash what the resolver authenticated.
///
/// "One hash per requested object" has no counter - a hash is not charged
/// anywhere - so the property is pinned where it lives: the wave compares the
/// identity the resolver returned instead of recomputing it over the same bytes.
/// A second `ObjectId::for_bytes` in this file is the defect returning, and this
/// case fails closed on it, the same shape the pooled-lane ceiling guard uses.
#[test]
fn the_read_wave_does_not_hash_what_the_resolver_authenticated() {
    let source = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/cas/read.rs"),
    )
    .expect("read wave source");
    assert!(
        !source.contains("for_bytes"),
        "the wave hashes resolved bytes again instead of using the identity the resolver authenticated"
    );
    assert!(
        source.contains("verified != *id"),
        "the wave still checks the resolved identity, one comparison instead of one hash"
    );
}

/// A locator whose identity does not match its bytes is still refused.
///
/// The requested object is hashed once per wave (P2-6): the resolver computes the
/// identity to authenticate the record against the locator and hands it back, and
/// the wave compares identities instead of hashing the same bytes again. This case
/// is the tamper detection that must not weaken - a row whose `object_id` names
/// bytes that hash to something else is refused - plus its control, the same
/// object read successfully before the row is rewritten.
#[test]
fn a_locator_whose_identity_does_not_match_its_bytes_is_refused() {
    let dir = TempDir::new("identity");
    let path = dir.store_path("identity");
    let bytes = patterned(40_000);
    let (collected, root, _) = construct_file(&bytes);
    let store = create_store(&path);
    save_all(&store, &collected).expect("save");

    let (values, _) = read_objects(&store, &[root]).expect("the stored object reads");
    assert_eq!(values.len(), 1);
    assert_eq!(
        layerfs_content::ObjectId::for_bytes(&values[0]),
        root,
        "the control reads the object the locator names"
    );

    // Rewrite the locator so it claims an identity whose bytes are different; the
    // record itself is untouched, so only the identity check can catch it.
    let connection = rusqlite::Connection::open(&path).expect("external connection");
    let claimed = layerfs_content::ObjectId::for_bytes(b"layerfs/phase2/p2-6/claimed");
    let affected = connection
        .execute(
            "UPDATE objects SET object_id = ?1 WHERE object_id = ?2",
            rusqlite::params![claimed.to_bytes().to_vec(), root.to_bytes().to_vec()],
        )
        .expect("external rewrite");
    assert_eq!(affected, 1, "the locator was rewritten");
    drop(connection);

    let reopened = open_store(&path);
    match read_objects(&reopened, &[claimed]) {
        Err(StorageError::Integrity(what)) => {
            assert!(
                what.contains("identity"),
                "an identity mismatch is reported as one, got {what}"
            );
        }
        Err(other) => panic!("expected an identity refusal, got {other}"),
        Ok(_) => panic!("a locator whose bytes hash to another identity was accepted"),
    }
}

/// The presence-query counter: one charge per offered object that has to ask.
///
/// A wave seeds its availability with the identities it *offered*; a direct
/// reference outside that set - an object already stored but not part of this
/// wave - costs one paged presence query. The counter is what the wave-level
/// batch (`P2-5`) is measured against, so it is pinned here with its control: a
/// wave whose references are all known pays nothing.
#[test]
fn a_presence_query_is_charged_for_a_reference_outside_the_wave() {
    let dir = TempDir::new("presence");
    let path = dir.store_path("presence");
    let store = create_store(&path);

    let target = layerfs_content::FinalizedObject::new(
        ObjectRole::WholeFile,
        support::assembled_small_object(b"presence-target"),
    )
    .expect("target object");
    let target_id = target.id();
    let first = disabled(|scope| {
        let mut operation = store.begin_save(scope.child("begin"))?;
        operation.accept(target)?;
        operation.finish(scope.child("finish"))
    })
    .expect("target save");
    assert_eq!(first.inserted, 1);
    assert_eq!(
        first.presence_queries, 0,
        "a wave with no outside reference asks nothing"
    );

    // A second object that names the stored target: the target is available but
    // was not offered in this wave, so availability has to ask once.
    let dependent = layerfs_content::FinalizedObject::new(
        ObjectRole::WholeFile,
        support::assembled_small_object(b"presence-dependent"),
    )
    .expect("dependent object")
    .with_references(vec![target_id]);
    let second = disabled(|scope| {
        let mut operation = store.begin_save(scope.child("begin"))?;
        operation.accept(dependent)?;
        operation.finish(scope.child("finish"))
    })
    .expect("dependent save");
    assert_eq!(second.inserted, 1);
    assert_eq!(
        second.presence_queries, 1,
        "one outside reference is one presence query"
    );

    // Control: offering the reference itself in the wave makes it known without a
    // query, so the charge follows the reference, not the save.
    let offered = layerfs_content::FinalizedObject::new(
        ObjectRole::WholeFile,
        support::assembled_small_object(b"presence-offered"),
    )
    .expect("offered object")
    .with_references(vec![target_id]);
    let third = disabled(|scope| {
        let mut operation = store.begin_save(scope.child("begin"))?;
        operation.accept(offered)?;
        operation.finish(scope.child("finish"))
    })
    .expect("third save");
    assert_eq!(third.presence_queries, 1, "the reference is still outside");
}

/// A canonical chunk object whose payload is highly compressible and distinct per
/// `marker`.
///
/// Compressibility is the point: the encoded record is small, so a group stays
/// under its framed target and remains open across a preparation wave boundary.
fn compressible_chunk(marker: u32) -> FinalizedObject {
    let mut payload = vec![0u8; 8 * 1024];
    payload[..4].copy_from_slice(&marker.to_be_bytes());
    FinalizedObject::new(
        ObjectRole::Chunk,
        layerfs_content::file::mapping::encode_chunk_object(&payload).expect("canonical chunk"),
    )
    .expect("finalized chunk")
}

#[test]
fn a_group_sealed_mid_wave_answers_its_other_members_from_that_wave() {
    let dir = TempDir::new("sealed_mid_wave");
    let path = dir.store_path("sealed_mid_wave");
    let a = compressible_chunk(1);
    let b = compressible_chunk(2);
    assert_ne!(a.id(), b.id());

    // Two distinct identities, offered alternately, span several preparation
    // waves. Because their records compress, both sit in one open group when the
    // first wave ends. The next wave re-offers the first of them, which seals that
    // group and writes a row for each member; the second member must then be
    // answered from the row that seal wrote. Before the fix it was treated as
    // absent for the rest of the wave and offered a second time, and the `objects`
    // primary key refused the row.
    let store = create_store(&path);
    let outcome = disabled(|scope| {
        let mut operation = store.begin_save(scope.child("storage.begin"))?;
        for index in 0..200u32 {
            operation.accept(if index % 2 == 0 { a.clone() } else { b.clone() })?;
        }
        operation.finish(scope.child("storage.finish"))
    })
    .expect("a wave that seals a group must not offer its members twice");

    assert_eq!(outcome.inserted, 2, "one row per distinct identity");
    assert_eq!(
        outcome.reused, 198,
        "every other occurrence is served by the row that exists"
    );
    assert_eq!(outcome.packs_created, 1, "both members share one pack");

    let (values, _) = read_objects(&store, &[a.id(), b.id()]).unwrap();
    assert_eq!(values[0], a.canonical());
    assert_eq!(values[1], b.canonical());
}
