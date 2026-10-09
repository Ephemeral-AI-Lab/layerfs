//! What holds a closed Workspace. A descriptor, a lookup owner and a native
//! connection with its kernel lookup counts each hold it alone, with no
//! `lease` row standing for them, until the last one is released; nothing
//! held queues its cleanup at Close; and rows of another Workspace never
//! hold it. The owner test reaches every custody table by its key.
use layerfs_overlay::{
    CleanupState, Inode, InodeKind, MaintenanceCursor, NativeMountState, Overlay, ProfileConfig,
    Route,
};
use std::path::PathBuf;

struct Temp(PathBuf);
impl Drop for Temp {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}
fn engine() -> (Temp, Overlay) {
    let path = std::env::temp_dir().join(format!("layerfs-close-custody-{}", std::process::id()));
    std::fs::create_dir(&path).unwrap();
    let db = Overlay::create(&path.join("overlay"), ProfileConfig::default()).unwrap();
    (Temp(path), db)
}
/// A regular file of the immutable base.
fn base(serial: u64) -> Inode {
    Inode {
        serial,
        kind: InodeKind::File,
        mode: 0o644,
        mtime_seconds: 1,
        mtime_nanoseconds: 0,
        nlink: 1,
        size: 0,
        inherited_cutoff: 0,
        born: 0,
        entries: 0,
        subdirs: 0,
    }
}
fn state(db: &Overlay, route: Route) -> CleanupState {
    db.cleanup_state(route).unwrap()
}
fn maintain(db: &Overlay) {
    let mut cursor = MaintenanceCursor::default();
    for _ in 0..200 {
        let Some(step) = db.maintain(cursor).unwrap() else {
            return;
        };
        cursor = step.cursor;
    }
    panic!("maintenance did not settle");
}

#[test]
fn each_kind_of_custody_alone_holds_a_closed_workspace_and_another_workspace_never_does() {
    let (_temp, db) = engine();
    let workspace = |tag: u8| db.open_workspace([tag; 32], [9; 32]).unwrap();

    // A bystander that owns every kind of custody throughout.
    let bystander = workspace(1);
    let source = db.acquire_base_source(bystander, 1).unwrap();
    let kept_file = db.open_file(source, 1, &base(7), true).unwrap();
    let kept_lookup = db.acquire_lookup(source, 2, &base(8), 1).unwrap();
    db.release_base_source(source).unwrap();
    let kept_mount = db.create_native_mount(bystander, 1).unwrap();
    let counts = db.resources(Some(bystander)).unwrap().counts;
    // No `lease` row stands for a descriptor or a lookup reference: the
    // rows are the handle, the lookup owner, the mount and its root count,
    // the root's parent and the custody rows of the three inodes.
    assert_eq!((counts.owner_rows, counts.owner_details), (0, 8));

    // Nothing held: Close queues the cleanup itself.
    let none = workspace(2);
    db.close(none).unwrap();
    assert_eq!(state(&db, none), CleanupState::Queued);

    // A descriptor alone.
    let route = workspace(3);
    let source = db.acquire_base_source(route, 1).unwrap();
    let file = db.open_file(source, 1, &base(7), false).unwrap();
    db.release_base_source(source).unwrap();
    db.close(route).unwrap();
    assert_eq!(state(&db, route), CleanupState::Held);
    db.close_file(file).unwrap();
    assert_eq!(state(&db, route), CleanupState::Queued);

    // A lookup owner alone.
    let route = workspace(4);
    let source = db.acquire_base_source(route, 1).unwrap();
    let lookup = db.acquire_lookup(source, 1, &base(7), 1).unwrap();
    db.release_base_source(source).unwrap();
    db.close(route).unwrap();
    assert_eq!(state(&db, route), CleanupState::Held);
    db.release_lookup(lookup).unwrap();
    assert_eq!(state(&db, route), CleanupState::Queued);

    // A native connection alone: its row and its root's kernel count.
    let route = workspace(5);
    let mount = db.create_native_mount(route, 1).unwrap();
    db.close(route).unwrap();
    assert_eq!(state(&db, route), CleanupState::Held);
    db.revoke_native_mount(mount).unwrap();
    assert_eq!(state(&db, route), CleanupState::Held);
    maintain(&db);
    assert_eq!(state(&db, route), CleanupState::Queued);

    // None of that touched the bystander, and its rows held none of them.
    assert_eq!(state(&db, bystander), CleanupState::Live);
    db.check_file(kept_file, true).unwrap();
    db.check_lookup(kept_lookup).unwrap();
    assert_eq!(
        db.native_mount_state(kept_mount).unwrap(),
        NativeMountState::Live
    );
    assert_eq!(db.resources(Some(bystander)).unwrap().counts, counts);

    // Closed, it is held until the last of its owners is released.
    db.close(bystander).unwrap();
    assert_eq!(state(&db, bystander), CleanupState::Held);
    db.close_file(kept_file).unwrap();
    assert_eq!(state(&db, bystander), CleanupState::Held);
    db.release_lookup(kept_lookup).unwrap();
    assert_eq!(state(&db, bystander), CleanupState::Held);
    db.revoke_native_mount(kept_mount).unwrap();
    maintain(&db);
    assert_eq!(state(&db, bystander), CleanupState::Queued);
    let left = db.resources(Some(bystander)).unwrap().counts;
    assert_eq!((left.owner_rows, left.owner_details), (0, 0));
}

#[test]
fn the_owner_test_of_a_closed_workspace_reaches_every_custody_table_by_key() {
    let (_temp, db) = engine();
    let plans = db.explain_close_held().unwrap();
    println!("CLOSE_HELD_PLANS {plans:#?}");
    for table in [
        "lease",
        "file_handle",
        "lookup_owner",
        "native_lookup",
        "native_mount",
        "native_directory",
    ] {
        assert!(
            plans
                .iter()
                .any(|plan| plan.starts_with(&format!("SEARCH {table} "))),
            "{table}: {plans:?}"
        );
    }
    // The statement selects from no table: its one outer row is constant.
    assert!(
        plans.iter().all(|plan| {
            (!plan.contains("SCAN") || plan == "SCAN CONSTANT ROW") && !plan.contains("TEMP B-TREE")
        }),
        "{plans:?}"
    );
}
