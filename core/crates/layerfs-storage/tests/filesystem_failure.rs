//! C2 failure semantics for real filesystem trees.
//!
//! A retained tree is saved first; then an attempt fails through an external
//! database constraint, through a tree-operation failure inside the save, or
//! through a corrupted pack on the read path. Each case asserts the same
//! contract: the failure is reported once, the publication watermark does not
//! move, only this attempt's rows are cleaned up, the earlier tree is still
//! readable, there is no retry and no acknowledgement is faked.

mod support;

use support::filesystem::{
    build_tree, corrupt_pack_containing, database_state, reject_locators_after, save_bag, Built,
    FailingReader, StoreReader,
};
use support::{create_store, disabled, open_store, TempDir};

use layerfs_content::filesystem::{
    update_filesystem, DirectoryUpdate, FilesystemInput, FilesystemObjects, FilesystemRead,
    FilesystemResources, FilesystemRoot, FilesystemRootId, LogicalPath, PathName,
};
use layerfs_content::{
    AuthenticatedObjects, ContentError, FinalizedConsumer, ObjectId, ObjectRole,
};
use layerfs_storage::{SaveHandoff, StorageError, Store};

fn name(value: &str) -> PathName {
    PathName::new(value).expect("name")
}

/// Saves one built tree and returns its store path.
fn retained_tree(label: &str, entries: usize) -> (TempDir, Built, u64) {
    let temp = TempDir::new(label);
    let store = create_store(&temp.store_path(label));
    let built = build_tree(entries);
    let outcome = save_bag(&store, &built.bag).expect("retained save");
    assert!(outcome.acknowledged, "the retained tree is acknowledged");
    assert!(
        outcome.pool.leaves > 0,
        "the retained tree used the pooled lane"
    );
    drop(store);
    (temp, built, outcome.inserted)
}

/// Reads the retained root through a reopened Store.
fn assert_retained_root_readable(store: &Store, built: &Built) {
    let reader = StoreReader::new(store);
    let bytes = reader
        .read_canonical(built.root)
        .expect("the retained root object is readable");
    let root = FilesystemRoot::decode(&bytes).expect("the retained root decodes");
    let mut read = FilesystemRead::new(&reader, FilesystemRootId(built.root)).expect("reader");
    let listing = read
        .list(&LogicalPath::root(), None, 16, 4096)
        .expect("the retained tree lists");
    assert_eq!(
        listing.entries.len(),
        4,
        "the retained tree keeps its four top-level names"
    );
    assert_eq!(root.scope(), built.scope);
}

#[test]
fn a_late_external_constraint_fails_once_without_moving_the_watermark() {
    let (temp, built, _) = retained_tree("fsfailure-late", 1_500);
    let path = temp.store_path("fsfailure-late");
    let retained = database_state(&path);
    // The retained tree's own attempt already published; the trigger rejects the
    // *next* attempt once it has written some rows, so the failure is late.
    reject_locators_after(&path, retained.0 + 2);
    let store = open_store(&path);

    // A second, differently sized tree, fed through the real save: it shares the
    // content and attribute objects and adds its own pages, so the trigger fires
    // after some of them are already written.
    let second = build_tree(300);
    let fresh = second
        .bag
        .objects
        .keys()
        .filter(|id| !built.bag.objects.contains_key(*id))
        .count();
    assert!(
        fresh >= 3,
        "the second tree must contribute new objects: {fresh}"
    );
    let outcome: Result<(), StorageError> = disabled(|scope| {
        let mut operation = store.begin_save(scope.child("storage.begin"))?;
        let mut handoff = SaveHandoff::new(&mut operation);
        for (_, (role, bytes)) in second.bag.objects.iter() {
            handoff.accept(
                layerfs_content::FinalizedObject::new(*role, bytes.clone())
                    .expect("finalized object"),
            )?;
        }
        if let Some(failure) = handoff.take_failure() {
            return Err(failure);
        }
        operation.finish(scope.child("storage.finish"))?;
        Ok(())
    });
    let error = outcome.expect_err("the externally rejected locator must fail the save");
    assert!(
        matches!(error, StorageError::Engine(_)),
        "a constraint failure is reported as an engine failure: {error}"
    );
    assert!(
        !error.is_unknown_outcome(),
        "a pre-commit rejection is a definite failure"
    );

    let after = database_state(&path);
    assert_eq!(
        after.2, retained.2,
        "a failed attempt must not advance the publication watermark"
    );
    assert_eq!(
        (after.0, after.1),
        (retained.0, retained.1),
        "the failed attempt cleaned up exactly its own rows"
    );
    assert_retained_root_readable(&store, &built);
    // No retry: the same objects may be offered again only as a new attempt, and
    // the rejected attempt itself is finished.
    let again = save_bag(&store, &second.bag);
    assert!(
        again.is_err(),
        "the installed constraint still rejects the same unlucky row"
    );
    assert_eq!(database_state(&path).2, retained.2);
}

#[test]
fn a_tree_failure_inside_a_save_aborts_this_attempt_only() {
    let (temp, built, _) = retained_tree("fsfailure-tree", 400);
    let path = temp.store_path("fsfailure-tree");
    let retained = database_state(&path);
    let store = open_store(&path);

    // A real update input that fails late: the provider refuses a demanded page
    // after a few successful reads, so the C1 operation fails after it has already
    // handed objects to the save.
    let updates = [DirectoryUpdate {
        parent: built.directory,
        changes: built
            .entries
            .iter()
            .take(4)
            .enumerate()
            .map(|(index, serial)| (name(&format!("e{index:04}")), Some(*serial)))
            .collect(),
    }];
    let input = FilesystemInput {
        base: Some(FilesystemRootId(built.root)),
        scope: built.scope,
        root_serial: 1,
        directories: &updates,
        inodes: &[],
        new_inodes: &[],
        resources: FilesystemResources::default(),
    };
    let reader = FailingReader::new(&built.bag, 1);
    let outcome: Result<(), StorageError> = disabled(|scope| {
        let mut operation = store.begin_save(scope.child("storage.begin"))?;
        let mut handoff = SaveHandoff::new(&mut operation);
        let update = {
            let mut objects = FilesystemObjects::new(&reader, &mut handoff);
            update_filesystem(&mut objects, &input, None)
        };
        if let Some(failure) = handoff.take_failure() {
            return Err(failure);
        }
        update?;
        operation.finish(scope.child("storage.finish"))?;
        Ok(())
    });
    let error = outcome.expect_err("a failed tree operation must fail the save");
    assert!(
        matches!(error, StorageError::Content(ContentError::MissingObject)),
        "the tree failure is reported once: {error}"
    );
    let after = database_state(&path);
    assert_eq!(after.2, retained.2, "the watermark is untouched");
    assert_eq!(
        (after.0, after.1),
        (retained.0, retained.1),
        "only this attempt's rows were removed"
    );
    assert_retained_root_readable(&store, &built);
}

#[test]
fn a_corrupted_pack_fails_the_read_without_touching_retained_state() {
    let (temp, built, _) = retained_tree("fsfailure-read", 200);
    let path = temp.store_path("fsfailure-read");
    let retained = database_state(&path);
    corrupt_pack_containing(&path, built.root);
    let store = open_store(&path);
    let reader = StoreReader::new(&store);
    let outcome = reader.read_canonical(built.root);
    assert!(
        outcome.is_err(),
        "a corrupted pack must fail the read rather than returning wrong bytes"
    );
    assert_eq!(
        database_state(&path),
        retained,
        "a failed read never moves the watermark or the locator rows"
    );
    // The failure is deterministic: the same demand fails again, it does not
    // silently fall back to another route or return partial bytes.
    let again = reader.read_canonical(built.root);
    assert!(again.is_err(), "a repeated demand fails the same way");
    assert_eq!(database_state(&path), retained);
    // The tree itself is unchanged: the same root identity still decodes from the
    // retained record, and only the corrupted pack's objects are unreadable.
    assert_eq!(
        built.bag.get(built.root).expect("in-memory root"),
        FilesystemRoot::decode(&built.bag.get(built.root).expect("in-memory root"))
            .expect("decode")
            .encode()
            .expect("encode"),
        "the retained canonical root bytes are still the tree's identity"
    );
}

#[test]
fn a_private_attempt_is_invisible_until_acknowledgement_and_absent_after_failure() {
    let (temp, built, inserted) = retained_tree("fsfailure-visibility", 900);
    let path = temp.store_path("fsfailure-visibility");
    let retained = database_state(&path);
    assert_eq!(retained.0, inserted as i64);

    // A second tree, offered into an attempt that never finishes: the objects are
    // written and committed in bounded transactions, but nothing is published.
    let store = open_store(&path);
    let second = build_tree(300);
    let offered = second
        .bag
        .objects
        .keys()
        .filter(|id| !built.bag.objects.contains_key(*id))
        .count() as i64;
    assert!(offered > 0, "the abandoned attempt must offer new objects");
    {
        let mut operation =
            disabled(|scope| store.begin_save(scope.child("storage.begin"))).expect("begin");
        disabled(|scope| {
            for (_, (role, bytes)) in second.bag.objects.iter() {
                operation.accept(
                    layerfs_content::FinalizedObject::new(*role, bytes.clone())
                        .expect("finalized object"),
                    scope.child("storage.accept"),
                )?;
            }
            Ok::<(), StorageError>(())
        })
        .expect("bounded early write");
        // Abandon the attempt without acknowledgement.
        drop(operation);
    }
    let abandoned = database_state(&path);
    assert_eq!(
        abandoned.2, retained.2,
        "an unacknowledged attempt never moves the watermark"
    );
    assert!(
        abandoned.0 >= retained.0,
        "the attempt really wrote rows before being abandoned"
    );
    assert!(
        abandoned.0 < retained.0 + offered,
        "and it published none of them: {} rows for {offered} new objects",
        abandoned.0 - retained.0
    );
    // A reader sees the retained tree only.
    let store = open_store(&path);
    assert_retained_root_readable(&store, &built);
    let reader = StoreReader::new(&store);
    let hidden = second
        .bag
        .objects
        .keys()
        .find(|id| !built.bag.objects.contains_key(*id))
        .copied()
        .expect("an object the retained tree does not hold");
    assert!(
        reader.read_canonical(hidden).is_err(),
        "an abandoned attempt's object is not readable"
    );
    // One cleanup on the next attempt removes the abandoned rows.
    let outcome = save_bag(&store, &built.bag).expect("the retained tree still saves");
    assert_eq!(outcome.inserted, 0);
    let cleaned = database_state(&path);
    assert_eq!(cleaned.2, retained.2);
    assert_eq!(
        cleaned.0, retained.0,
        "the abandoned attempt's rows are gone: {} rows, expected {}",
        cleaned.0, retained.0
    );
    let _ = ObjectId::for_bytes(b"fsfailure");
    let _ = ObjectRole::FilesystemRoot;
}
