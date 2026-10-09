//! One row per open descriptor. A native descriptor's row names its mount
//! and the kernel request that received it, so a repeated request identity
//! is refused whole, a lost visit's descriptor is found by its request, and
//! neither another mount's rows nor a caller's own request of the same
//! number ever answer for it. Retiring a mount reads its own descriptors
//! only, however many others the engine holds.
use layerfs_overlay::{
    Inode, InodeKind, MaintenanceCursor, NativeDecision, NativeMount, NativeMountState, OpenFile,
    Overlay, OverlayError, ProfileConfig, Route,
};
use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

struct Temp(PathBuf);
impl Drop for Temp {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}
fn engine() -> (Temp, Overlay) {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    let path = std::env::temp_dir().join(format!(
        "layerfs-descriptor-request-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
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
/// A mounted Workspace whose kernel holds one lookup count on file 50.
fn mounted(db: &Overlay, tag: u8) -> (Route, NativeMount) {
    let route = db.open_workspace([tag; 32], [9; 32]).unwrap();
    let mount = db.create_native_mount(route, 1).unwrap();
    let source = db.acquire_native_source(mount, 1, 1).unwrap();
    let looked = db.observe_native(mount, source, true, |_, _| {
        Ok(NativeDecision::Finished {
            inode: Some(base(50)),
            value: (),
        })
    });
    db.release_file_read(looked.result.unwrap().unwrap())
        .unwrap();
    db.release_base_source(source).unwrap();
    (route, mount)
}
/// OPEN of file 50, received by kernel request `request` of `mount`.
fn open(db: &Overlay, mount: NativeMount, request: u64) -> Result<OpenFile, OverlayError> {
    let opened = db.open_native_visit(mount, request, 50, false, |_, _| {
        Ok(NativeDecision::Finished {
            inode: Some(base(50)),
            value: (),
        })
    });
    opened.result?;
    Ok(opened.open_candidate.expect("the OPEN's descriptor"))
}
/// A descriptor opened for a caller's own request, outside any mount.
fn plain(db: &Overlay, route: Route, request: u64) -> Result<OpenFile, OverlayError> {
    let source = db.acquire_base_source(route, 1).unwrap();
    let file = db.open_file(source, request, &base(50), false);
    db.release_base_source(source).unwrap();
    file
}

#[test]
fn a_request_owns_one_descriptor_of_its_mount_and_no_other_row_answers_for_it() {
    let (_temp, db) = engine();
    let (route, mount) = mounted(&db, 1);
    let (other_route, other_mount) = mounted(&db, 2);
    let rows = |route| db.resources(Some(route)).unwrap().counts;

    let before = rows(route);
    let first = open(&db, mount, 10).unwrap();
    let held = rows(route);
    // The descriptor is one row and no `lease` row; the file's custody row
    // already stood for the kernel's lookup count.
    assert_eq!(
        (
            held.owner_rows - before.owner_rows,
            held.owner_details - before.owner_details
        ),
        (0, 1)
    );
    assert_eq!(db.retained_native_file(mount, 10).unwrap(), Some(first));

    // The same request identity again: refused whole, nothing written.
    let again = open(&db, mount, 10);
    assert!(matches!(again, Err(OverlayError::Sql(_))), "{again:?}");
    assert_eq!(rows(route), held);
    assert_eq!(db.retained_native_file(mount, 10).unwrap(), Some(first));
    db.check_file(first, false).unwrap();

    // Another request of the mount, and the same number in another mounted
    // Workspace, each own a descriptor of their own.
    let second = open(&db, mount, 11).unwrap();
    let other = open(&db, other_mount, 10).unwrap();
    assert_eq!(db.retained_native_file(mount, 11).unwrap(), Some(second));
    assert_eq!(
        db.retained_native_file(other_mount, 10).unwrap(),
        Some(other)
    );
    assert_eq!(db.retained_native_file(mount, 10).unwrap(), Some(first));
    assert_eq!(db.retained_native_file(other_mount, 11).unwrap(), None);
    // A descriptor answers only through the mount that received it.
    for (mount, file) in [(mount, other), (other_mount, first), (other_mount, second)] {
        assert!(matches!(
            db.native_file(mount, 50, file.owner_id()),
            Err(OverlayError::Stale)
        ));
        assert!(matches!(
            db.close_native_file(mount, 50, file.owner_id()),
            Err(OverlayError::Stale)
        ));
    }

    // A caller's own request of the same number is a different identity,
    // refused when it repeats, and never found through the mount.
    assert_eq!(db.retained_file(route, 10).unwrap(), None);
    let caller = plain(&db, route, 10).unwrap();
    assert_eq!(db.retained_file(route, 10).unwrap(), Some(caller));
    assert_eq!(db.retained_native_file(mount, 10).unwrap(), Some(first));
    let repeated = plain(&db, route, 10);
    assert!(
        matches!(repeated, Err(OverlayError::Sql(_))),
        "{repeated:?}"
    );
    assert_eq!(db.retained_file(other_route, 10).unwrap(), None);
    assert!(matches!(
        db.native_file(mount, 50, caller.owner_id()),
        Err(OverlayError::Stale)
    ));

    // RELEASE removes the one row; the request no longer finds it.
    db.close_native_file(mount, 50, first.owner_id()).unwrap();
    assert_eq!(db.retained_native_file(mount, 10).unwrap(), None);
    assert_eq!(db.retained_file(route, 10).unwrap(), Some(caller));
    assert_eq!(
        rows(route).owner_details - before.owner_details,
        2,
        "the second native descriptor and the caller's"
    );
    assert_eq!(rows(other_route).owner_details - before.owner_details, 1);
}

#[test]
fn retiring_a_mount_reads_its_own_descriptors_however_many_others_exist() {
    const OWN: u64 = 3;
    let retire = |others: u64| {
        let (_temp, db) = engine();
        let (route, mount) = mounted(&db, 1);
        let (_, other_mount) = mounted(&db, 2);
        let own: Vec<OpenFile> = (0..OWN)
            .map(|index| open(&db, mount, 100 + index).unwrap())
            .collect();
        let callers: Vec<OpenFile> = (0..others)
            .map(|index| plain(&db, route, 100 + index).unwrap())
            .collect();
        let foreign: Vec<OpenFile> = (0..others)
            .map(|index| open(&db, other_mount, 100 + index).unwrap())
            .collect();

        let before = db.diagnostics();
        db.revoke_native_mount(mount).unwrap();
        let mut cursor = MaintenanceCursor::default();
        for _ in 0..200 {
            let Some(step) = db.maintain(cursor).unwrap() else {
                break;
            };
            cursor = step.cursor;
        }
        let work = db.diagnostics().since(&before).total();
        assert_eq!(
            db.native_mount_state(mount).unwrap(),
            NativeMountState::Gone
        );
        // Its own descriptors are gone; every other one is untouched.
        for file in own {
            assert!(matches!(
                db.check_file(file, false),
                Err(OverlayError::Stale)
            ));
        }
        for (index, file) in callers.iter().enumerate() {
            db.check_file(*file, false).unwrap();
            assert_eq!(
                db.retained_file(route, 100 + index as u64).unwrap(),
                Some(*file)
            );
        }
        for (index, file) in foreign.iter().enumerate() {
            assert_eq!(
                db.retained_native_file(other_mount, 100 + index as u64)
                    .unwrap(),
                Some(*file)
            );
        }
        assert_eq!((work.fullscan_steps, work.sorts), (0, 0), "{work:?}");
        (work.attempts, work.executions, work.vm_steps)
    };
    let (few, many) = (retire(2), retire(40));
    println!("DESCRIPTOR_RETIRE own={OWN} beside_2={few:?} beside_40={many:?}");
    assert_eq!(few, many);
}
