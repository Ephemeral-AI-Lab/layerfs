//! Failure semantics: unavailable ownership, missing dependencies, late failure
//! after early acknowledged writes, one cleanup attempt and preserved data.

mod support;

use layerfs_content::{
    construct_stream, ConstructionPolicy, ContentError, FinalizedObject, ObjectRole,
};
use layerfs_storage::{SaveHandoff, StorageError};
use support::{
    construct_file, create_store, disabled, noise, open_store, patterned, read_objects, save_all,
    TempDir,
};

fn store_counts(path: &std::path::Path) -> (i64, i64) {
    let connection = rusqlite::Connection::open(path).unwrap();
    let objects: i64 = connection
        .query_row("SELECT COUNT(*) FROM objects", [], |row| row.get(0))
        .unwrap();
    let packs: i64 = connection
        .query_row("SELECT COUNT(*) FROM object_packs", [], |row| row.get(0))
        .unwrap();
    (objects, packs)
}

#[test]
fn a_second_save_cannot_acquire_ownership_while_the_first_holds_it() {
    let dir = TempDir::new("owner");
    let path = dir.store_path("owner");
    let store = create_store(&path);
    let holder = disabled(|scope| store.begin_save(scope.child("storage.begin"))).unwrap();
    let second = disabled(|scope| store.begin_save(scope.child("storage.begin")));
    assert!(
        matches!(second, Err(StorageError::OwnershipUnavailable)),
        "a lost write lock must fail immediately"
    );
    drop(holder);
    let third = disabled(|scope| store.begin_save(scope.child("storage.begin")));
    assert!(third.is_ok(), "ownership is available once it is released");
}

#[test]
fn a_missing_direct_dependency_fails_without_writing_a_parent() {
    let dir = TempDir::new("dependency");
    let path = dir.store_path("dependency");
    let store = create_store(&path);
    let bytes = noise(131_072 * 2);
    let (collected, _, _) = construct_file(&bytes);
    let leaf = collected
        .objects()
        .iter()
        .find(|(_, role, _, _)| *role == ObjectRole::ExtentLeaf)
        .map(|(_, role, raw, references)| {
            FinalizedObject::new(*role, raw.clone())
                .unwrap()
                .with_references(references.clone())
        })
        .expect("an extent leaf exists");

    let error = disabled(|scope| {
        let mut operation = store.begin_save(scope.child("storage.begin"))?;
        operation.accept(leaf, scope.child("storage.accept"))?;
        operation.finish(scope.child("storage.finish"))
    })
    .unwrap_err();
    assert!(
        matches!(error, StorageError::MissingDependency { .. }),
        "got {error}"
    );
    let (objects, packs) = store_counts(&path);
    assert_eq!((objects, packs), (0, 0), "nothing was left behind");
}

#[test]
fn a_late_input_failure_after_an_early_commit_cleans_up_once_and_keeps_earlier_objects() {
    let dir = TempDir::new("late");
    let path = dir.store_path("late");
    let store = create_store(&path);

    // A retained object set that must survive the failed attempt untouched.
    let retained_bytes = patterned(9_000);
    let (retained, retained_root, _) = construct_file(&retained_bytes);
    save_all(&store, &retained).expect("retained save");
    let retained_counts = store_counts(&path);

    // A large streamed file whose source fails after most of its chunks were
    // accepted and after at least one bounded transaction was acknowledged.
    let body = noise(4 * 1024 * 1024);
    let failing = support::FailingAfter::new(body.clone(), body.len() - 4_096);
    let policy = ConstructionPolicy::frozen_default();
    let outcome: Result<(), StorageError> = disabled(|scope| {
        let mut operation = store.begin_save(scope.child("storage.begin"))?;
        let mut handoff = SaveHandoff::new(&mut operation);
        let construction = construct_stream(
            policy,
            &policy.capacities(),
            failing,
            &mut handoff,
            scope.child("content"),
        );
        if let Some(failure) = handoff.take_failure() {
            return Err(failure);
        }
        construction?;
        operation.finish(scope.child("storage.finish"))?;
        Ok(())
    });
    let error = outcome.unwrap_err();
    assert!(
        matches!(error, StorageError::Content(ContentError::Io)),
        "the original input failure is reported once: {error}"
    );

    let after = store_counts(&path);
    assert_eq!(
        after, retained_counts,
        "the definite failure removed its own rows and kept the retained ones"
    );

    let (values, _) = read_objects(&store, &[retained_root]).unwrap();
    assert_eq!(values[0], retained.objects()[0].2);

    // The failed attempt already ran its one cleanup; dropping the operation and
    // storing again must not remove anything else.
    let again = save_all(&store, &retained).expect("the retained object still saves/reuses");
    assert_eq!(again.inserted, 0);
    assert_eq!(store_counts(&path), retained_counts);
}

#[test]
fn a_terminal_operation_refuses_further_work() {
    let dir = TempDir::new("terminal");
    let path = dir.store_path("terminal");
    let store = create_store(&path);
    let bytes = noise(131_072 * 2);
    let (collected, _, _) = construct_file(&bytes);
    let leaf = collected
        .objects()
        .iter()
        .find(|(_, role, _, _)| *role == ObjectRole::ExtentLeaf)
        .map(|(_, role, raw, references)| {
            FinalizedObject::new(*role, raw.clone())
                .unwrap()
                .with_references(references.clone())
        })
        .expect("a chunked file has an extent leaf");

    let mut operation = disabled(|scope| store.begin_save(scope.child("storage.begin"))).unwrap();
    disabled(|scope| operation.accept(leaf, scope.child("storage.accept"))).unwrap();

    // Fill the bounded batch so its preparation wave runs; the leaf's unresolved
    // dependency is then reported by the operation.
    let mut error = None;
    for index in 0..40u8 {
        let payload = support::repeat(64 * 1024, index);
        let object = FinalizedObject::new(
            ObjectRole::WholeFile,
            assembled_small(&payload[..64 * 1024 - 1]),
        )
        .unwrap();
        match disabled(|scope| operation.accept(object, scope.child("storage.accept"))) {
            Ok(()) => {}
            Err(failure) => {
                error = Some(failure);
                break;
            }
        }
    }
    let error = error.expect("a full batch prepares and reports the missing dependency");
    assert!(
        matches!(error, StorageError::MissingDependency { .. }),
        "got {error}"
    );

    let second = disabled(|scope| {
        operation.accept(
            FinalizedObject::new(ObjectRole::WholeFile, assembled_small(b"x")).unwrap(),
            scope.child("storage.accept"),
        )
    });
    assert!(
        matches!(second, Err(StorageError::Aborted)),
        "got {second:?}"
    );
    let finish = disabled(|scope| operation.finish(scope.child("storage.finish")));
    assert!(
        matches!(finish, Err(StorageError::Aborted)),
        "got {finish:?}"
    );
}

/// Builds a canonical whole-file object without going through C1 construction.
fn assembled_small(raw: &[u8]) -> Vec<u8> {
    let mut value = Vec::new();
    value.extend_from_slice(b"LFS5SML\0");
    value.extend_from_slice(&1u16.to_be_bytes());
    value.extend_from_slice(raw);
    layerfs_content::object::codec::encode_bytes_object(&value).unwrap()
}

#[test]
fn a_real_constraint_failure_is_reported_once_and_preserves_retained_objects() {
    let dir = TempDir::new("sqlfail");
    let path = dir.store_path("sqlfail");
    let store = create_store(&path);
    let (first, first_root, _) = construct_file(&patterned(500));
    save_all(&store, &first).expect("first save");
    drop(store);

    // An externally installed database constraint that rejects the locator
    // insert. It is ordinary database behaviour, not a product fault switch.
    {
        let connection = rusqlite::Connection::open(&path).unwrap();
        connection
            .execute_batch(
                "CREATE TRIGGER reject_locators BEFORE INSERT ON objects \
                 BEGIN SELECT RAISE(ABORT, 'externally rejected locator'); END;",
            )
            .unwrap();
    }

    let reopened = open_store(&path);
    let (second, _, _) = construct_file(&patterned(600));
    let error = save_all(&reopened, &second).unwrap_err();
    assert!(
        matches!(
            error,
            StorageError::Engine(rusqlite::Error::SqliteFailure(_, _))
        ),
        "got {error}"
    );
    assert!(
        !error.is_unknown_outcome(),
        "a pre-commit failure is definite"
    );

    let (values, _) = read_objects(&reopened, &[first_root]).expect("retained object is intact");
    assert_eq!(values[0], first.objects()[0].2);
    let (objects, packs) = store_counts(&path);
    assert_eq!(
        (objects, packs),
        (1, 1),
        "the failed attempt left nothing behind"
    );
}

#[test]
fn an_unknown_write_acknowledgement_cannot_be_induced_without_a_fault_hook() {
    // A lost `COMMIT` acknowledgement is reachable only through an engine or
    // transport failure. The product path returns it as
    // `StorageError::UnknownOutcome` from `sqlite::write::commit`, quarantines the
    // save and never resends, polls or deletes. This slice permits no fault
    // injection in `src/`, so the condition is not induced here: it stays an
    // explicit coverage gap rather than a fabricated passing result.
    let dir = TempDir::new("unknown");
    let path = dir.store_path("unknown");
    let store = create_store(&path);
    let operation = disabled(|scope| store.begin_save(scope.child("storage.begin"))).unwrap();
    let outcome = disabled(|scope| operation.finish(scope.child("storage.finish"))).unwrap();
    assert_eq!(outcome.commits, 0);
    assert_eq!(outcome.inserted, 0);
}

#[test]
fn reading_a_store_that_was_removed_underneath_fails_cleanly() {
    let dir = TempDir::new("removed");
    let path = dir.store_path("removed");
    let store = create_store(&path);
    let (collected, root, _) = construct_file(&patterned(700));
    save_all(&store, &collected).unwrap();
    std::fs::remove_file(&path).unwrap();
    let error = disabled(|scope| store.read_batch(&[root], scope.child("storage.read")));
    assert!(
        matches!(error, Err(StorageError::Engine(_))),
        "got {error:?}"
    );
}
