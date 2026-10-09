//! Native directory offsets, source custody and bounded live cleanup.
use layerfs_overlay::{
    CleanupState, Inode, InodeKind, MaintenanceCursor, NativeDecision, NativeDirectory,
    NativeDirectoryCursor, NativeMount, Overlay, OverlayError, ProfileConfig,
};
use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
struct Fixture {
    db: Overlay,
    mount: NativeMount,
    directory: NativeDirectory,
    path: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let path = std::env::temp_dir().join(format!(
            "layerfs-native-directory-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        let db = Overlay::create(&path.join("overlay"), ProfileConfig::default()).unwrap();
        let route = db.open_workspace([81; 32], [82; 32]).unwrap();
        let mount = db.create_native_mount(route, 1).unwrap();
        let source = db.acquire_native_source(mount, u64::MAX, 1).unwrap();
        let original = db.observe_native_directory(mount, source, |_, serial| {
            Ok(NativeDecision::Finished {
                inode: Some(Inode {
                    serial,
                    kind: InodeKind::Directory,
                    mode: 0o755,
                    mtime_seconds: 1,
                    mtime_nanoseconds: 0,
                    nlink: 0,
                    size: 0,
                    inherited_cutoff: 0,
                    born: 0,
                    entries: 0,
                    subdirs: 0,
                }),
                value: serial,
            })
        });
        let read = original.result.unwrap().unwrap();
        let directory = original.directory_candidate.unwrap();
        assert!(original.open_candidate.is_none());
        db.release_file_read(read).unwrap();
        db.release_base_source(source).unwrap();
        Self {
            db,
            mount,
            directory,
            path,
        }
    }
    fn read(&self, request: u64, offset: u64) -> layerfs_overlay::NativeDirectoryRead {
        self.db
            .acquire_native_directory_read(self.directory, request, offset)
            .unwrap()
    }
    fn finish(&self, read: layerfs_overlay::NativeDirectoryRead) {
        self.db.release_base_source(read.source()).unwrap();
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}
fn names(values: &[&str]) -> Vec<Vec<u8>> {
    values
        .iter()
        .map(|value| value.as_bytes().to_vec())
        .collect()
}

#[test]
fn only_accepted_entries_publish_offsets_and_concurrent_aliases_remain_valid() {
    let f = Fixture::new();
    assert_eq!(f.db.native_lookup_count(f.mount, 1).unwrap(), Some(0));
    assert_eq!(
        f.db.retained_native_directory(f.mount, u64::MAX).unwrap(),
        Some(f.directory)
    );
    assert!(f
        .db
        .native_directory(f.mount, 2, f.directory.owner_id())
        .is_err());
    let foreign = Fixture::new();
    assert!(f
        .db
        .native_directory(foreign.mount, 1, f.directory.owner_id())
        .is_err());
    let a = f.read(1, 0);
    assert_eq!(*a.cursor(), NativeDirectoryCursor::Start);
    assert_eq!(a.parent(), 1);
    let b = f.read(2, 2);
    assert_eq!(*b.cursor(), NativeDirectoryCursor::Names(None));
    let pa =
        f.db.prepare_native_cookies(&a, &names(&["a", "b", "c"]))
            .unwrap();
    let pb =
        f.db.prepare_native_cookies(&b, &names(&["a", "b"]))
            .unwrap();
    assert_ne!(pa.entries()[0].cookie(), pb.entries()[0].cookie());
    assert!(f
        .db
        .acquire_native_directory_read(f.directory, 99, pa.entries()[0].cookie())
        .is_err());
    f.db.publish_native_cookies(&pa, 1).unwrap();
    f.db.publish_native_cookies(&pb, 2).unwrap();
    assert!(matches!(
        f.db.publish_native_cookies(&pa, 1),
        Err(OverlayError::Stale)
    ));
    assert!(f.db.prepare_native_cookies(&a, &names(&["d"])).is_err());
    assert!(f
        .db
        .acquire_native_directory_read(f.directory, 98, pa.entries()[1].cookie())
        .is_err());
    let old = f.read(3, pa.entries()[0].cookie());
    assert_eq!(old.cursor().after_name(), Some(b"a".as_slice()));
    assert_eq!(
        f.db.retained_native_directory_read(f.mount, 3).unwrap(),
        Some(old.clone())
    );
    let alias = f.read(4, pb.entries()[0].cookie());
    assert_eq!(alias.cursor().after_name(), Some(b"a".as_slice()));
    let rewind = f.read(5, 2);
    let reuse =
        f.db.prepare_native_cookies(&rewind, &names(&["a", "b", "d"]))
            .unwrap();
    assert_eq!(reuse.entries()[0].cookie(), pa.entries()[0].cookie());
    assert_eq!(reuse.entries()[1].cookie(), pb.entries()[1].cookie());
    f.db.publish_native_cookies(&reuse, 0).unwrap();
    assert!(f
        .db
        .acquire_native_directory_read(f.directory, 97, reuse.entries()[2].cookie())
        .is_err());
    let dot = f.read(6, 1);
    assert_eq!(*dot.cursor(), NativeDirectoryCursor::AfterDot);
    f.db.close_native_directory(f.mount, f.directory.serial(), f.directory.owner_id())
        .unwrap();
    assert!(f
        .db
        .close_native_directory(f.mount, f.directory.serial(), f.directory.owner_id())
        .is_err());
    assert!(f
        .db
        .acquire_native_directory_read(f.directory, 7, 0)
        .is_err());
    assert!(f.db.revoke_native_mount(f.mount).is_err());
    // The already received source can still compose and publish an EOF reply.
    assert!(f
        .db
        .native_directory_page(&dot, None)
        .unwrap()
        .local
        .active
        .is_empty());
    let empty = f.db.prepare_native_cookies(&dot, &[]).unwrap();
    f.db.publish_native_cookies(&empty, 0).unwrap();
    for read in [a, b, old, alias, rewind, dot] {
        f.finish(read);
    }
    // Closed-cookie rows do not prevent logical mount revocation/close.
    f.db.revoke_native_mount(f.mount).unwrap();
    f.db.close(f.mount.route()).unwrap();
    assert_eq!(
        f.db.cleanup_state(f.mount.route()).unwrap(),
        CleanupState::Held
    );
    let mut cursor = MaintenanceCursor::default();
    for _ in 0..100 {
        if let Some(step) = f.db.maintain(cursor).unwrap() {
            cursor = step.cursor;
            assert!(step.work.rows <= 64);
        }
        if f.db.cleanup_state(f.mount.route()).unwrap() == CleanupState::Queued {
            break;
        }
    }
    for _ in 0..100 {
        if f.db.cleanup_state(f.mount.route()).unwrap() == CleanupState::Gone {
            break;
        }
        f.db.reclaim_closed(0).unwrap();
    }
    assert_eq!(
        f.db.cleanup_state(f.mount.route()).unwrap(),
        CleanupState::Gone
    );
    let counts = f.db.resources(None).unwrap().counts;
    assert_eq!(
        (
            counts.owner_details,
            counts.owner_rows,
            counts.source_rows,
            counts.namespaces
        ),
        (0, 0, 0, 0)
    );
}

#[test]
fn cookies_are_reused_and_retire_in_bounded_indexed_live_turns() {
    let f = Fixture::new();
    let plans = f.db.explain_native_directory(f.directory).unwrap();
    assert!(
        plans.iter().all(|plan| plan.contains("SEARCH")),
        "{plans:?}"
    );
    assert!(
        plans
            .iter()
            .any(|plan| plan.contains("open-fence") && plan.contains("native_directory_open")),
        "{plans:?}"
    );
    println!("NATIVE_DIRECTORY_PLANS {plans:?}");
    let before = f.db.diagnostics();
    let mut last = 2;
    for (batch, range) in [(0, 0..64), (1, 64..128), (2, 128..130)] {
        let read = f.read(batch + 1, last);
        let input: Vec<_> = range.map(|n| format!("n{n:03}").into_bytes()).collect();
        let plan = f.db.prepare_native_cookies(&read, &input).unwrap();
        f.db.publish_native_cookies(&plan, input.len()).unwrap();
        last = plan.entries().last().unwrap().cookie();
        f.finish(read);
    }
    let populated = f.db.resources(Some(f.mount.route())).unwrap().counts;
    let read = f.read(4, 2);
    let input: Vec<_> = (0..64).map(|n| format!("n{n:03}").into_bytes()).collect();
    let plan = f.db.prepare_native_cookies(&read, &input).unwrap();
    f.db.publish_native_cookies(&plan, 64).unwrap();
    f.finish(read);
    assert_eq!(
        f.db.resources(Some(f.mount.route())).unwrap().counts,
        populated,
        "published-name rewind allocates no new cookie rows"
    );
    let held = f.read(5, last);
    f.db.close_native_directory(f.mount, f.directory.serial(), f.directory.owner_id())
        .unwrap();
    let mut cursor = MaintenanceCursor::default();
    for _ in 0..4 {
        if let Some(step) = f.db.maintain(cursor).unwrap() {
            cursor = step.cursor;
        }
    }
    assert!(f
        .db
        .retained_native_directory(f.mount, u64::MAX)
        .unwrap()
        .is_some());
    f.finish(held);
    let mut windows = Vec::new();
    for _ in 0..30 {
        if let Some(step) = f.db.maintain(cursor).unwrap() {
            cursor = step.cursor;
            windows.push(step.work.rows);
            assert!(step.work.rows <= 64);
        }
        if f.db
            .retained_native_directory(f.mount, u64::MAX)
            .unwrap()
            .is_none()
        {
            break;
        }
    }
    assert!(
        windows.iter().filter(|rows| **rows == 64).count() >= 2,
        "{windows:?}"
    );
    assert_eq!(
        f.db.retained_native_directory(f.mount, u64::MAX).unwrap(),
        None
    );
    assert_eq!(
        f.db.native_lookup_count(f.mount, 1).unwrap(),
        Some(0),
        "mount remains live"
    );
    let work = f.db.diagnostics().since(&before);
    assert_eq!(work.total().fullscan_steps, 0);
    assert_eq!(work.total().sorts, 0);
    assert_eq!(work.total().reprepares, 0);
    println!("NATIVE_DIRECTORY_WORK {work:?} windows={windows:?}");
    f.db.revoke_native_mount(f.mount).unwrap();
}
