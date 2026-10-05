//! Exact transient base-source custody through public engine APIs.
use layerfs_overlay::*;
use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let p = std::env::temp_dir().join(format!(
            "layerfs-source-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&p).unwrap();
        Self(p)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}
fn inode() -> Inode {
    Inode {
        serial: 2,
        kind: InodeKind::File,
        mode: 0o644,
        mtime_seconds: 4,
        mtime_nanoseconds: 5,
        nlink: 1,
        size: 7,
        inherited_cutoff: 0,
        born: 0,
        entries: 0,
    }
}
#[test]
fn source_retains_selected_base_and_exact_close_custody_without_a_generation_pin() {
    let temp = Temp::new();
    let db = Overlay::create(&temp.0.join("db"), ProfileConfig::default()).unwrap();
    let route = db.open_workspace([1; 32], [2; 32]).unwrap();
    let source = db.acquire_base_source(route, 7).unwrap();
    assert_eq!(source.root(), [2; 32]);
    assert_eq!(db.retained_base_source(route, 7).unwrap(), Some(source));
    assert!(db.acquire_base_source(route, 7).is_err());
    assert_eq!(db.state(route).unwrap().base_readers, 1);
    let p = db.publish(route, &inode(), None, None).unwrap();
    db.reply_attempted(p).unwrap();
    let capture = db.capture(route).unwrap();
    let before = db.state(route).unwrap();
    assert!(!db.install_ready(capture).unwrap());
    assert!(matches!(
        db.install(capture, [3; 32]),
        Err(OverlayError::BaseSourcesPending)
    ));
    assert_eq!(db.state(route).unwrap(), before);
    assert_eq!(db.source_inode(source, 2).unwrap(), Some(inode()));
    db.close(route).unwrap();
    assert!(matches!(
        db.acquire_base_source(route, 8),
        Err(OverlayError::Closed)
    ));
    db.release_closed_capture(capture).unwrap();
    assert_eq!(db.cleanup_state(route).unwrap(), CleanupState::Held);
    assert!(db.reclaim_closed(0).unwrap().is_none());
    assert_eq!(db.source_inode(source, 2).unwrap(), Some(inode()));
    db.release_base_source(source).unwrap();
    assert_eq!(db.state(route).unwrap().base_readers, 0);
    assert_eq!(db.cleanup_state(route).unwrap(), CleanupState::Queued);
    assert!(matches!(
        db.release_base_source(source),
        Err(OverlayError::Stale)
    ));
    while db.reclaim_closed(0).unwrap().is_some() {}
    assert_eq!(db.cleanup_state(route).unwrap(), CleanupState::Gone);
}
#[test]
fn source_owner_observation_keeps_indexed_work_as_unrelated_custody_grows() {
    let temp = Temp::new();
    let db = Overlay::create(&temp.0.join("db"), ProfileConfig::default()).unwrap();
    let route = db.open_workspace([11; 32], [12; 32]).unwrap();
    let other = db.open_workspace([13; 32], [12; 32]).unwrap();
    let source = db.acquire_base_source(route, 7).unwrap();
    let plan = db.explain_base_source(route, 7).unwrap();
    assert!(
        plan.iter()
            .all(|p| p.contains("SEARCH") && !p.contains("TEMP B-TREE")),
        "{plan:?}"
    );
    let mut previous = 0;
    let mut vm = None;
    for count in [128, 1024, 4096] {
        for owner in previous + 1..=count {
            db.acquire_base_source(other, owner).unwrap();
        }
        previous = count;
        let before = db.diagnostics().statements[StatementKind::Lease as usize];
        assert_eq!(db.retained_base_source(route, 7).unwrap(), Some(source));
        let after = db.diagnostics().statements[StatementKind::Lease as usize];
        let steps = after.vm_steps - before.vm_steps;
        if let Some(old) = vm {
            assert_eq!(old, steps);
        }
        vm = Some(steps);
        assert_eq!(after.attempts - before.attempts, 1);
        assert_eq!(after.rows_returned - before.rows_returned, 1);
        assert_eq!(after.fullscan_steps - before.fullscan_steps, 0);
        assert_eq!(after.sorts - before.sorts, 0);
        assert_eq!(after.autoindex_rows - before.autoindex_rows, 0);
        assert_eq!(after.reprepares - before.reprepares, 0);
        assert_eq!(db.state(other).unwrap().base_readers, count);
        println!("BASE_SOURCE unrelated_owners={count} point_vm={steps} returned=1 fullscan=0 sorts=0 autoindex=0 reprepare=0 plan={plan:?}");
    }
}

#[test]
fn source_changes_have_finite_plans_and_correlated_runtime_work() {
    let temp = Temp::new();
    let db = Overlay::create(&temp.0.join("db"), ProfileConfig::default()).unwrap();
    let route = db.open_workspace([21; 32], [22; 32]).unwrap();
    let plans = db.explain_base_source_changes(route, 7).unwrap();
    assert!(
        plans
            .iter()
            .filter(|p| !p.starts_with("insert:"))
            .all(|p| p.contains("SEARCH") && !p.contains("TEMP B-TREE")),
        "{plans:?}"
    );
    assert!(
        !plans
            .iter()
            .any(|p| p.contains(" Rewind ") || p.contains(" Next ") || p.contains(" SorterOpen ")),
        "{plans:?}"
    );
    let before = db.diagnostics();
    let source = db.acquire_base_source(route, 7).unwrap();
    let acquired = db.diagnostics();
    db.release_base_source(source).unwrap();
    let released = db.diagnostics();
    for (phase, start, end) in [
        ("acquire", before, acquired),
        ("release", acquired, released),
    ] {
        let mut total_vm = 0;
        let mut total_rows = 0;
        for family in [
            StatementKind::Begin,
            StatementKind::Commit,
            StatementKind::Workspace,
            StatementKind::Lease,
        ] {
            let a = start.statements[family as usize];
            let b = end.statements[family as usize];
            assert_eq!(b.fullscan_steps - a.fullscan_steps, 0);
            assert_eq!(b.sorts - a.sorts, 0);
            assert_eq!(b.autoindex_rows - a.autoindex_rows, 0);
            assert_eq!(b.reprepares - a.reprepares, 0);
            total_vm += b.vm_steps - a.vm_steps;
            total_rows += b.rows_changed - a.rows_changed;
            println!("BASE_SOURCE_CHANGE phase={phase} family={family:?} runs={} vm={} returned={} changed={} bound_bytes={}",b.executions-a.executions,b.vm_steps-a.vm_steps,b.rows_returned-a.rows_returned,b.rows_changed-a.rows_changed,b.bound_bytes-a.bound_bytes);
        }
        assert_eq!(total_rows, 2);
        println!("BASE_SOURCE_CHANGE phase={phase} total_vm={total_vm} changed={total_rows} fullscan=0 sorts=0 plans={plans:?}");
    }
    assert_eq!(db.state(route).unwrap().base_readers, 0);
}

#[test]
fn ordered_source_name_windows_visit_only_parent_and_generation_with_runtime_evidence() {
    let temp = Temp::new();
    let db = Overlay::create(&temp.0.join("db"), ProfileConfig::default()).unwrap();
    let route = db.open_workspace([31; 32], [32; 32]).unwrap();
    let other = db.open_workspace([33; 32], [32; 32]).unwrap();
    for n in 0..193 {
        let p = db
            .publish(
                route,
                &inode(),
                Some(&Dentry {
                    parent: 1,
                    name: format!("n{n:04}").into_bytes(),
                    serial: Some(2),
                }),
                None,
            )
            .unwrap();
        db.reply_attempted(p).unwrap();
    }
    let capture = db.capture(route).unwrap();
    let source = db.acquire_base_source(route, 7).unwrap();
    let mut previous = 0;
    let mut vm = None;
    for count in [128, 1024, 4096] {
        for n in previous..count {
            let p = db
                .publish(
                    other,
                    &inode(),
                    Some(&Dentry {
                        parent: 1,
                        name: format!("n{n:04}").into_bytes(),
                        serial: Some(2),
                    }),
                    None,
                )
                .unwrap();
            db.reply_attempted(p).unwrap();
        }
        previous = count;
        let plan = db
            .explain_source_names(source, 1, capture.generation, Some(b"n0063"))
            .unwrap();
        assert!(
            plan.iter().all(|p| p.contains("SEARCH")
                && p.contains("dentry_capture")
                && !p.contains("TEMP B-TREE")),
            "{plan:?}"
        );
        let before = db.diagnostics();
        let window = db.source_name_window(source, 1, Some(b"n0063")).unwrap();
        let after = db.diagnostics();
        assert!(window.active.is_empty());
        assert_eq!(window.captured.len(), 64);
        assert_eq!(window.captured[0].name, b"n0064");
        assert_eq!(window.captured[63].name, b"n0127");
        let a = before.statements[StatementKind::Dentry as usize];
        let b = after.statements[StatementKind::Dentry as usize];
        let steps = b.vm_steps - a.vm_steps;
        let total_vm: u64 = after
            .statements
            .iter()
            .zip(before.statements)
            .map(|(end, start)| end.vm_steps - start.vm_steps)
            .sum();
        let total_runs: u64 = after
            .statements
            .iter()
            .zip(before.statements)
            .map(|(end, start)| end.executions - start.executions)
            .sum();
        if let Some(old) = vm {
            assert_eq!(old, steps);
        }
        vm = Some(steps);
        assert_eq!(b.rows_returned - a.rows_returned, 64);
        assert_eq!(b.fullscan_steps - a.fullscan_steps, 0);
        assert_eq!(b.sorts - a.sorts, 0);
        assert_eq!(b.autoindex_rows - a.autoindex_rows, 0);
        assert_eq!(b.reprepares - a.reprepares, 0);
        println!("S3_NAME_WINDOW unrelated_names={count} name_vm={steps} returned=64 total_vm={total_vm} total_runs={total_runs} source_checks_and_parent_are_separate_families fullscan=0 sorts=0 plan={plan:?}");
    }
    db.release_base_source(source).unwrap();
}
