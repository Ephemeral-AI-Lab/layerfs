//! Public custody proofs: retained identities, bounded observations and refusals.
use layerfs_overlay::*;
use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let path = std::env::temp_dir().join(format!(
            "layerfs-frontier-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn db(&self) -> PathBuf {
        self.0.join("db")
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}
fn inode(serial: u64) -> Inode {
    Inode {
        serial,
        kind: InodeKind::File,
        mode: 0o644,
        mtime_seconds: 4,
        mtime_nanoseconds: 5,
        nlink: 1,
        size: 4096,
        inherited_cutoff: 0,
        born: 0,
        entries: 0,
        subdirs: 0,
    }
}
fn cell(byte: u8) -> Cell {
    Cell {
        offset: 0,
        data: Box::new([byte; CELL_BYTES]),
        validity: Box::new([255; MASK_BYTES]),
    }
}
fn work(db: &Overlay, kind: StatementKind) -> StatementWork {
    db.diagnostics().statements[kind as usize]
}

#[test]
fn frozen_capture_and_tickets_reject_changed_identity_and_survive_close() {
    let temp = Temp::new();
    let db = Overlay::create(&temp.db(), ProfileConfig::default()).unwrap();
    let route = db.open_workspace([1; 32], [2; 32]).unwrap();
    assert_eq!(db.retained_capture(route).unwrap(), None);
    let first = db.publish(route, &inode(2), None, Some(&cell(17))).unwrap();
    assert_eq!(db.pending_publications(route, 0).unwrap(), [first]);
    db.reply_attempted(first).unwrap();
    let capture = db.capture(route).unwrap();
    let mut later_inode = inode(2);
    later_inode.size = 8192;
    let later = db
        .publish(route, &later_inode, None, Some(&cell(19)))
        .unwrap();
    let before = db.state(route).unwrap();
    assert_eq!(before.captured_revision, Some(capture.revision));
    assert!(before.revision > capture.revision);
    assert_eq!(db.retained_capture(route).unwrap(), Some(capture));
    assert_eq!(db.captured_inodes(capture, 0).unwrap(), [inode(2)]);
    assert_eq!(db.captured_cell(capture, 2, 0).unwrap(), Some(cell(17)));
    let mut wrong = capture;
    wrong.revision += 1;
    assert!(matches!(
        db.install(wrong, [3; 32]),
        Err(OverlayError::Stale)
    ));
    assert!(matches!(
        db.captured_inodes(wrong, 0),
        Err(OverlayError::Stale)
    ));
    let mut wrong_root = capture;
    wrong_root.base_root = [4; 32];
    assert!(matches!(
        db.install(wrong_root, [3; 32]),
        Err(OverlayError::Stale)
    ));
    let mut wrong_ticket = later;
    wrong_ticket.generation = first.generation;
    assert!(matches!(
        db.reply_attempted(wrong_ticket),
        Err(OverlayError::Stale)
    ));
    assert_eq!(db.pending_publications(route, 0).unwrap(), [later]);
    assert_eq!(db.state(route).unwrap(), before);
    assert!(matches!(
        db.capture(route),
        Err(OverlayError::CaptureInFlight)
    ));
    db.close(route).unwrap();
    let closed = db.state(route).unwrap();
    assert!(matches!(
        db.release_closed_capture(wrong),
        Err(OverlayError::Stale)
    ));
    assert!(matches!(
        db.release_closed_capture(wrong_root),
        Err(OverlayError::Stale)
    ));
    assert_eq!(db.state(route).unwrap(), closed);
    assert_eq!(db.retained_capture(route).unwrap(), Some(capture));
    assert_eq!(db.captured_cell(capture, 2, 0).unwrap(), Some(cell(17)));
    assert!(db.reclaim_closed(0).unwrap().is_none());
    db.release_closed_capture(capture).unwrap();
    assert_eq!(db.retained_capture(route).unwrap(), None);
    assert!(matches!(
        db.captured_cell(capture, 2, 0),
        Err(OverlayError::Stale)
    ));
    assert_eq!(db.cleanup_state(route).unwrap(), CleanupState::Held);
    db.reply_attempted(later).unwrap();
    while db.reclaim_closed(0).unwrap().is_some() {}
    let replacement = db.open_workspace([1; 32], [7; 32]).unwrap();
    assert_ne!(replacement.namespace(), route.namespace());
    assert!(matches!(
        db.retained_capture(route),
        Err(OverlayError::Stale)
    ));
    assert!(matches!(
        db.pending_publications(route, 0),
        Err(OverlayError::Stale)
    ));
    assert_eq!(db.retained_capture(replacement).unwrap(), None);
}

#[test]
fn exact_frontier_observations_visit_only_their_fixed_domain() {
    let temp = Temp::new();
    let db = Overlay::create(&temp.db(), ProfileConfig::default()).unwrap();
    let route = db.open_workspace([10; 32], [11; 32]).unwrap();
    let other = db.open_workspace([12; 32], [11; 32]).unwrap();
    let first = db.publish(route, &inode(1), None, None).unwrap();
    db.reply_attempted(first).unwrap();
    let capture = db.capture(route).unwrap();
    for serial in 1..=193 {
        db.publish(route, &inode(serial), None, None).unwrap();
    }
    // Reply tickets are held in the engine's memory; only the retained
    // capture is a stored point query.
    let plans = [db.explain_retained_capture(route).unwrap()];
    assert!(
        plans
            .iter()
            .flatten()
            .all(|p| p.contains("SEARCH") && !p.contains("TEMP B-TREE")),
        "{plans:?}"
    );
    let active = db.active_generation(route).unwrap();
    let mut previous = 0;
    let mut counts = None;
    for population in [128, 1024, 4096] {
        for serial in previous + 1..=population {
            db.publish(other, &inode(serial), None, None).unwrap();
        }
        previous = population;
        let c_before = work(&db, StatementKind::Capture);
        let f_before = work(&db, StatementKind::Frontier);
        assert_eq!(db.retained_capture(route).unwrap(), Some(capture));
        let page = db.pending_publications(route, 128).unwrap();
        assert_eq!(page.len(), 64);
        assert_eq!((page[0].revision(), page[63].revision()), (129, 192));
        assert!(page
            .iter()
            .all(|p| p.route() == route && p.generation == active));
        let c_after = work(&db, StatementKind::Capture);
        let f_after = work(&db, StatementKind::Frontier);
        let current = (
            c_after.vm_steps - c_before.vm_steps,
            f_after.vm_steps - f_before.vm_steps,
        );
        if let Some(prior) = counts {
            assert_eq!(current, prior);
        }
        counts = Some(current);
        // The ticket page is read from memory: no Frontier statement runs.
        assert_eq!(f_after, f_before);
        assert_eq!(c_after.attempts - c_before.attempts, 1);
        assert_eq!(c_after.rows_returned - c_before.rows_returned, 1);
        assert_eq!(c_after.fullscan_steps - c_before.fullscan_steps, 0);
        assert_eq!(c_after.sorts - c_before.sorts, 0);
        assert_eq!(c_after.autoindex_rows - c_before.autoindex_rows, 0);
        assert_eq!(c_after.reprepares - c_before.reprepares, 0);
        println!("S2_FRONTIER other_rows={population} capture_vm={} ticket_page_vm={} returned=1/64 fullscan=0 sorts=0 autoindex=0 reprepare=0 plans={plans:?}",current.0,current.1);
    }
    let mut after = 0;
    let mut count = 0;
    loop {
        let page = db.pending_publications(route, after).unwrap();
        if page.is_empty() {
            break;
        }
        assert!(page.len() <= PAGE_ROWS);
        for p in page {
            assert!(p.revision() > after as i64);
            after = p.revision() as u64;
            count += 1;
        }
    }
    assert_eq!(count, 193);
    assert!(matches!(
        db.capture(route),
        Err(OverlayError::CaptureInFlight)
    ));
}

#[test]
fn equal_local_routing_keys_never_cross_engine_owners() {
    let a = Temp::new();
    let b = Temp::new();
    let first = Overlay::create(&a.db(), ProfileConfig::default()).unwrap();
    let second = Overlay::create(&b.db(), ProfileConfig::default()).unwrap();
    let route_a = first.open_workspace([41; 32], [42; 32]).unwrap();
    let route_b = second.open_workspace([41; 32], [42; 32]).unwrap();
    assert_eq!(route_a.namespace(), route_b.namespace());
    assert_ne!(route_a, route_b);
    let before = second.state(route_b).unwrap();
    assert!(matches!(second.state(route_a), Err(OverlayError::Stale)));
    assert!(matches!(
        second.retained_capture(route_a),
        Err(OverlayError::Stale)
    ));
    assert!(matches!(
        second.cleanup_state(route_a),
        Err(OverlayError::Stale)
    ));
    assert!(matches!(
        second.publish(route_a, &inode(1), None, None),
        Err(OverlayError::Stale)
    ));
    assert_eq!(second.state(route_b).unwrap(), before);
}
