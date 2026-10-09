//! Drain-qualified revocation: handles whose kernel release never arrived.
use layerfs_overlay::{
    CleanupState, Inode, InodeKind, MaintenanceCursor, NativeDecision, NativeMount,
    NativeMountState, Overlay, ProfileConfig,
};
use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
struct Fixture {
    db: Overlay,
    mount: NativeMount,
    path: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let path = std::env::temp_dir().join(format!(
            "layerfs-native-revocation-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        let db = Overlay::create(&path.join("overlay"), ProfileConfig::default()).unwrap();
        let route = db.open_workspace([91; 32], [92; 32]).unwrap();
        let mount = db.create_native_mount(route, 1).unwrap();
        Self { db, mount, path }
    }
    /// One kernel LOOKUP reference for a regular file below the root.
    fn lookup(&self, serial: u64) {
        let outcome = self
            .db
            .observe_native_visit(self.mount, 1, None, true, |_, _| {
                Ok(NativeDecision::Finished {
                    inode: Some(inode(serial, InodeKind::File)),
                    value: (),
                })
            });
        assert_eq!(outcome.result.unwrap(), None);
    }
    /// A completed OPEN whose RELEASE is never delivered.
    fn open_file(&self, request: u64, serial: u64) {
        let outcome = self
            .db
            .open_native_visit(self.mount, request, serial, false, |_, _| {
                Ok(NativeDecision::Finished {
                    inode: Some(inode(serial, InodeKind::File)),
                    value: (),
                })
            });
        assert_eq!(outcome.result.unwrap(), None);
        assert!(outcome.open_candidate.is_some());
    }
    /// A completed OPENDIR whose RELEASEDIR is never delivered.
    fn open_directory(&self, request: u64) {
        let outcome = self
            .db
            .opendir_native_visit(self.mount, request, 1, |_, _| {
                Ok(NativeDecision::Finished {
                    inode: Some(inode(1, InodeKind::Directory)),
                    value: (),
                })
            });
        assert_eq!(outcome.result.unwrap(), None);
        assert!(outcome.directory_candidate.is_some());
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}
fn inode(serial: u64, kind: InodeKind) -> Inode {
    Inode {
        serial,
        kind,
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

#[test]
fn revocation_retires_unreleased_handles_in_bounded_indexed_turns() {
    const HANDLES: u64 = 70;
    let f = Fixture::new();
    f.lookup(2);
    for index in 0..HANDLES {
        f.open_file(100 + index, 2);
        f.open_directory(1000 + index);
    }
    let plans = f.db.explain_native(f.mount).unwrap();
    assert!(
        plans.iter().all(|plan| plan.contains("SEARCH")),
        "{plans:?}"
    );
    assert!(plans.iter().any(|plan| plan.starts_with("retire-files")));
    assert!(plans.iter().any(
        |plan| plan.starts_with("retire-directories") && plan.contains("native_directory_open")
    ));
    // An open handle does not fence the mark.
    assert_eq!(
        f.db.native_mount_state(f.mount).unwrap(),
        NativeMountState::Live
    );
    let before = f.db.diagnostics();
    f.db.revoke_native_mount(f.mount).unwrap();
    let mark = f.db.diagnostics().since(&before);
    // The mark waits for no row: it reads the Workspace and the mount, then
    // writes the mark and its retirement entry. No request row is probed.
    assert_eq!(
        (mark.total().attempts, mark.total().executions),
        (7, 8),
        "{mark:?}"
    );
    assert_eq!(
        f.db.native_mount_state(f.mount).unwrap(),
        NativeMountState::Revoked
    );
    f.db.close(f.mount.route()).unwrap();
    assert_eq!(
        f.db.cleanup_state(f.mount.route()).unwrap(),
        CleanupState::Held,
        "unretired native ownership keeps namespace deletion held"
    );
    let mut cursor = MaintenanceCursor::default();
    let (mut turns, mut maximum_rows, mut rows) = (0_u64, 0, 0);
    for _ in 0..1000 {
        match f.db.maintain(cursor).unwrap() {
            Some(step) => {
                cursor = step.cursor;
                turns += 1;
                rows += step.work.rows;
                maximum_rows = maximum_rows.max(step.work.rows);
            }
            None => break,
        }
    }
    assert!(maximum_rows <= 64, "{maximum_rows}");
    assert!(rows >= 2 * HANDLES, "{rows}");
    assert!(turns >= 4, "{turns}");
    assert_eq!(
        f.db.native_mount_state(f.mount).unwrap(),
        NativeMountState::Gone
    );
    assert_eq!(
        f.db.cleanup_state(f.mount.route()).unwrap(),
        CleanupState::Queued
    );
    for _ in 0..1000 {
        if f.db.reclaim_closed(0).unwrap().is_none() {
            break;
        }
    }
    assert_eq!(
        f.db.cleanup_state(f.mount.route()).unwrap(),
        CleanupState::Gone
    );
    assert_eq!(f.db.resources(None).unwrap().counts, Default::default());
    let work = f.db.diagnostics().since(&before);
    assert_eq!(work.total().fullscan_steps, 0);
    assert_eq!(work.total().sorts, 0);
    println!(
        "NATIVE_HANDLE_RETIRE handles={} turns={turns} rows={rows} maximum_window={maximum_rows} mark={mark:?} work={work:?}",
        2 * HANDLES
    );
}
