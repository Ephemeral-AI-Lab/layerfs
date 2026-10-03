//! Barrier-controlled same-Store ownership; no timing claim or test-only hooks.
mod support;
use layerfs_storage::{StorageError, Store};
use support::{construct_file, create_store, disabled, noise, read_logical, TempDir};

#[test]
fn private_saves_overlap_and_publish_in_reverse_order_with_identical_bytes() {
    let temp = TempDir::new("multiwriter");
    let path = temp.store_path("shared");
    let store = create_store(&path);
    let bytes = noise(700_000);
    let (objects, root, _) = construct_file(&bytes);
    let opened = disabled(|s| Store::open(&path, s.child("open"))).unwrap();
    let mut a = disabled(|s| store.begin_save(s.child("a"))).unwrap();
    let mut b = disabled(|s| opened.begin_save(s.child("b"))).unwrap();
    assert!(matches!(
        disabled(|s| store.begin_save(s.child("full"))),
        Err(StorageError::OwnershipUnavailable)
    ));
    for object in objects.finalized() {
        a.accept(object).unwrap();
    }
    for object in objects.finalized() {
        b.accept(object).unwrap();
    }
    let private_id = objects.objects()[0].0;
    let a_private = disabled(|s| a.read_batch(&[private_id], s.child("private"))).unwrap();
    assert!(!a_private.is_empty());
    assert!(disabled(|s| store.contains(&[root], s.child("hidden")))
        .unwrap()
        .is_empty());
    let outcome_b = disabled(|s| b.finish(s.child("finish-b"))).unwrap();
    assert!(outcome_b.inserted > 0);
    assert_eq!(read_logical(&store, root), bytes);
    let outcome_a = disabled(|s| a.finish(s.child("finish-a"))).unwrap();
    assert!(outcome_a.inserted > 0);
    assert_eq!(read_logical(&opened, root), bytes);
    drop(store);
    drop(opened);
    let reopened = disabled(|s| Store::open(&path, s.child("reopen"))).unwrap();
    assert_eq!(read_logical(&reopened, root), bytes);
    let connection = rusqlite::Connection::open(&path).unwrap();
    let duplicated: i64 = connection
        .query_row(
            "SELECT count(*) FROM objects WHERE object_id=?1",
            [root.as_bytes()],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(duplicated, 2, "independent locator ownership is accepted");
}

#[test]
fn abort_removes_only_its_save_and_never_lends_a_foreign_dependency() {
    let temp = TempDir::new("multiwriter-abort");
    let store = create_store(&temp.store_path("shared"));
    let (a_objects, a_root, _) = construct_file(&noise(700_000));
    let bytes = vec![41; 200_000];
    let (b_objects, b_root, _) = construct_file(&bytes);
    let mut a = disabled(|s| store.begin_save(s.child("a"))).unwrap();
    let mut b = disabled(|s| store.begin_save(s.child("b"))).unwrap();
    for object in a_objects.finalized() {
        a.accept(object).unwrap();
    }
    let a_private = a_objects.objects()[0].0;
    disabled(|s| a.read_batch(&[a_private], s.child("seal-a"))).unwrap();
    for object in b_objects.finalized() {
        b.accept(object).unwrap();
    }
    disabled(|s| b.finish(s.child("finish-b"))).unwrap();
    assert!(
        disabled(|s| store.contains(&[a_private], s.child("private-hidden-after-b")))
            .unwrap()
            .is_empty()
    );
    disabled(|s| a.abort(s.child("abort"))).unwrap();
    assert!(disabled(|s| store.contains(&[a_root], s.child("a-hidden")))
        .unwrap()
        .is_empty());
    assert_eq!(read_logical(&store, b_root), bytes);
    let next = disabled(|s| store.begin_save(s.child("slot-reclaimed"))).unwrap();
    disabled(|s| next.abort(s.child("abort"))).unwrap();
}

#[test]
fn separate_threads_prepare_and_finish_the_same_content_successfully() {
    let temp = TempDir::new("multiwriter-threads");
    let store = create_store(&temp.store_path("shared"));
    let bytes = noise(700_000);
    let (objects, root, _) = construct_file(&bytes);
    let barrier = std::sync::Barrier::new(2);
    std::thread::scope(|threads| {
        let run = || {
            let mut save = disabled(|s| store.begin_save(s.child("begin"))).unwrap();
            barrier.wait();
            for object in objects.finalized() {
                save.accept(object).unwrap();
            }
            disabled(|s| save.finish(s.child("finish"))).unwrap()
        };
        let a = threads.spawn(run);
        let b = threads.spawn(run);
        assert!(a.join().unwrap().inserted > 0);
        assert!(b.join().unwrap().inserted > 0);
    });
    assert_eq!(read_logical(&store, root), bytes);
}

#[test]
fn unpublished_foreign_content_cannot_satisfy_a_dependency() {
    let temp = TempDir::new("multiwriter-dependency");
    let store = create_store(&temp.store_path("shared"));
    let bytes = noise(700_000);
    let (objects, root, _) = construct_file(&bytes);
    let mut a = disabled(|s| store.begin_save(s.child("a"))).unwrap();
    let mut b = disabled(|s| store.begin_save(s.child("b"))).unwrap();
    for object in objects.finalized() {
        a.accept(object).unwrap();
    }
    let private = objects.objects()[0].0;
    disabled(|s| a.read_batch(&[private], s.child("seal"))).unwrap();
    let (other, _, _) = construct_file(b"other");
    b.accept(other.finalized().remove(0).with_references(vec![private]))
        .unwrap();
    assert!(matches!(
        disabled(|s| b.finish(s.child("finish"))),
        Err(StorageError::MissingDependency { .. })
    ));
    disabled(|s| a.finish(s.child("finish"))).unwrap();
    assert_eq!(read_logical(&store, root), bytes);
}

#[test]
fn an_older_schema_is_refused_without_promotion() {
    let temp = TempDir::new("multiwriter-schema");
    let path = temp.store_path("old");
    drop(create_store(&path));
    let connection = rusqlite::Connection::open(&path).unwrap();
    connection.execute_batch("PRAGMA user_version=6").unwrap();
    assert!(matches!(
        disabled(|s| Store::open(&path, s.child("open"))),
        Err(StorageError::UnsupportedPolicy {
            field: "schema identity"
        })
    ));
    let version: i64 = connection
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap();
    assert_eq!(version, 6);
}
