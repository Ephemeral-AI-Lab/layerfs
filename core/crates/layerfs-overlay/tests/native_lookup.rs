//! Public engine native ownership, atomicity and bounded indexed retirement.
use layerfs_overlay::{
    CleanupState, Inode, InodeKind, MaintenanceCursor, NativeDecision, NativeMount,
    NativeMountState, Overlay, OverlayError, ProfileConfig, StatementKind,
};
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
};
struct Temp(PathBuf);
impl Drop for Temp {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}
struct Fixture {
    db: Overlay,
    mount: NativeMount,
    _temp: Temp,
}
impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let path = std::env::temp_dir().join(format!(
            "layerfs-native-count-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        let db = Overlay::create(&path.join("overlay"), ProfileConfig::default()).unwrap();
        let route = db.open_workspace([21; 32], [22; 32]).unwrap();
        let mount = db.create_native_mount(route, 1).unwrap();
        Self {
            db,
            mount,
            _temp: Temp(path),
        }
    }
    fn lookup(&self, request: u64, serial: u64) {
        let source = self
            .db
            .acquire_native_source(self.mount, request, 1)
            .unwrap();
        let outcome = self
            .db
            .observe_native(self.mount, source, true, |_, protected| {
                assert_eq!(protected, 1);
                Ok(NativeDecision::Finished {
                    inode: Some(inode(serial)),
                    value: serial,
                })
            });
        assert_eq!(outcome.decision, Some(serial));
        let read = outcome.result.unwrap().unwrap();
        assert_eq!(outcome.candidate, Some(read));
        assert_eq!(
            self.db.retained_native_read(self.mount, request).unwrap(),
            Some(read)
        );
        self.db.release_file_read(read).unwrap();
        // This source already decided, even though its result reader is gone.
        assert!(matches!(
            self.db
                .observe_native(
                    self.mount,
                    source,
                    true,
                    |_, _| -> Result<NativeDecision<()>, OverlayError> {
                        panic!("replayed deciding job")
                    }
                )
                .result,
            Err(OverlayError::Stale)
        ));
        self.db.release_base_source(source).unwrap();
    }
}
fn inode(serial: u64) -> Inode {
    Inode {
        serial,
        kind: InodeKind::File,
        mode: 0o644,
        mtime_seconds: -2,
        mtime_nanoseconds: 3,
        nlink: 1,
        size: 0,
        inherited_cutoff: 0,
        born: 0,
        entries: 0,
    }
}

#[test]
fn aggregate_forget_is_checked_and_implicit_root_is_independent() {
    let f = Fixture::new();
    assert_eq!(f.db.native_lookup_count(f.mount, 1).unwrap(), Some(0));
    assert!(matches!(
        f.db.forget_native(f.mount, 1, 1),
        Err(OverlayError::Invalid("native lookup underflow"))
    ));
    f.lookup(1, 2);
    let rows = f.db.resources(Some(f.mount.route())).unwrap().counts;
    for request in 2..=32 {
        f.lookup(request, 2);
    }
    assert_eq!(f.db.native_lookup_count(f.mount, 2).unwrap(), Some(32));
    assert_eq!(
        f.db.resources(Some(f.mount.route())).unwrap().counts,
        rows,
        "repeated references add no ownership rows"
    );
    assert!(matches!(
        f.db.forget_native(f.mount, 2, 33),
        Err(OverlayError::Invalid("native lookup underflow"))
    ));
    assert_eq!(f.db.native_lookup_count(f.mount, 2).unwrap(), Some(32));
    f.db.forget_native(f.mount, 2, 31).unwrap();
    assert_eq!(f.db.native_lookup_count(f.mount, 2).unwrap(), Some(1));
    f.db.forget_native(f.mount, 2, 1).unwrap();
    assert_eq!(f.db.native_lookup_count(f.mount, 2).unwrap(), None);
    assert_eq!(f.db.native_lookup_count(f.mount, 1).unwrap(), Some(0));
    assert!(matches!(
        f.db.forget_native(f.mount, 2, 1),
        Err(OverlayError::Stale)
    ));
}

#[test]
fn source_and_read_fence_revocation_and_request_keys_keep_all_u64_bits() {
    let f = Fixture::new();
    let source = f.db.acquire_native_source(f.mount, u64::MAX, 1).unwrap();
    assert_eq!(
        f.db.retained_native_source(f.mount, u64::MAX).unwrap(),
        Some(source)
    );
    assert!(f.db.acquire_native_source(f.mount, u64::MAX, 1).is_err());
    assert!(matches!(
        f.db.revoke_native_mount(f.mount),
        Err(OverlayError::BaseSourcesPending)
    ));
    let outcome = f.db.observe_native(f.mount, source, true, |_, _| {
        Ok(NativeDecision::Finished {
            inode: Some(inode(2)),
            value: (),
        })
    });
    let read = outcome.result.unwrap().unwrap();
    f.db.release_base_source(source).unwrap();
    f.db.forget_native(f.mount, 2, 1).unwrap();
    assert!(f.db.source_inode(read.source(), 2).unwrap().is_none());
    assert_eq!(
        f.db.retained_native_read(f.mount, u64::MAX).unwrap(),
        Some(read)
    );
    assert!(matches!(
        f.db.revoke_native_mount(f.mount),
        Err(OverlayError::BaseSourcesPending)
    ));
    f.db.release_file_read(read).unwrap();
    f.db.revoke_native_mount(f.mount).unwrap();
    assert_eq!(
        f.db.native_mount_state(f.mount).unwrap(),
        NativeMountState::Revoked
    );
    assert!(matches!(
        f.db.acquire_native_source(f.mount, 4, 1),
        Err(OverlayError::Stale)
    ));
    assert!(matches!(
        f.db.revoke_native_mount(f.mount),
        Err(OverlayError::Stale)
    ));
}

#[test]
fn need_rounds_do_not_acquire_and_failed_atomic_decisions_keep_original_evidence() {
    let f = Fixture::new();
    let source = f.db.acquire_native_source(f.mount, 7, 1).unwrap();
    let before = f.db.resources(Some(f.mount.route())).unwrap().counts;
    let need = f.db.observe_native(f.mount, source, true, |_, _| {
        Ok(NativeDecision::Needs("base fact"))
    });
    assert_eq!(need.decision, Some("base fact"));
    assert!(matches!(need.result, Ok(None)));
    assert_eq!(need.candidate, None);
    assert_eq!(
        f.db.resources(Some(f.mount.route())).unwrap().counts,
        before
    );
    let original = Arc::new("original semantic value");
    let retained = original.clone();
    let start = f.db.diagnostics();
    let failed = f.db.observe_native(f.mount, source, true, |_, _| {
        Ok(NativeDecision::Finished {
            inode: Some(inode(0)),
            value: original,
        })
    });
    assert!(failed.result.is_err());
    assert!(Arc::ptr_eq(failed.decision.as_ref().unwrap(), &retained));
    assert_eq!(
        f.db.resources(Some(f.mount.route())).unwrap().counts,
        before
    );
    let work = f.db.diagnostics().since(&start);
    assert_eq!(
        work.statements[StatementKind::Rollback as usize].executions,
        1
    );
    assert_eq!(
        f.db.retained_native_source(f.mount, 7).unwrap(),
        Some(source)
    );
    f.db.release_base_source(source).unwrap();
}

#[test]
fn foreign_engine_or_mount_cannot_consume_ownership() {
    let f = Fixture::new();
    let other = Fixture::new();
    assert!(matches!(
        other.db.forget_native(f.mount, 1, 1),
        Err(OverlayError::Stale)
    ));
    let source = f.db.acquire_native_source(f.mount, 1, 1).unwrap();
    assert!(matches!(
        f.db.observe_native(
            other.mount,
            source,
            false,
            |_, _| -> Result<NativeDecision<()>, OverlayError> {
                panic!("foreign source reached semantics")
            }
        )
        .result,
        Err(OverlayError::Stale)
    ));
    f.db.release_base_source(source).unwrap();
}

#[test]
fn revoked_lookup_retirement_is_bounded_indexed_and_allows_closed_cleanup() {
    let f = Fixture::new();
    for serial in 2..=131 {
        f.lookup(serial, serial);
    }
    let plans = f.db.explain_native(f.mount).unwrap();
    assert!(
        plans.iter().all(|plan| plan.contains("SEARCH")),
        "{plans:?}"
    );
    assert!(plans
        .iter()
        .any(|plan| plan.contains("native_lookup") && plan.contains("serial>?")));
    println!("NATIVE_PLANS {plans:?}");
    f.db.close(f.mount.route()).unwrap();
    assert_eq!(
        f.db.cleanup_state(f.mount.route()).unwrap(),
        CleanupState::Held
    );
    f.db.revoke_native_mount(f.mount).unwrap();
    let start = f.db.diagnostics();
    let mut cursor = MaintenanceCursor::default();
    let mut maximum_rows = 0;
    for _ in 0..1000 {
        match f.db.maintain(cursor).unwrap() {
            Some(step) => {
                cursor = step.cursor;
                maximum_rows = maximum_rows.max(step.work.rows);
            }
            None => break,
        }
    }
    assert!(maximum_rows <= 64);
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
    println!(
        "NATIVE_RETIRE lookups=131 maximum_window={maximum_rows} work={:?}",
        f.db.diagnostics().since(&start)
    );
}
