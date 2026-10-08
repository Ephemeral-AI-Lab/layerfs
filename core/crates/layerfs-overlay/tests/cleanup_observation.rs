mod payload_support;
use layerfs_overlay::{CleanupState, OverlayError, StatementKind};
use payload_support::Temp;

#[test]
fn observation_keeps_live_and_held_state_and_reports_exact_physical_absence() {
    let temp = Temp::new();
    let db = temp.db();
    let incarnation = [71; 32];
    let route = db.open_workspace(incarnation, [72; 32]).unwrap();
    let ns = route.namespace();
    assert_eq!(
        db.observe_cleanup(ns, incarnation).unwrap(),
        CleanupState::Live
    );
    assert!(matches!(
        db.observe_cleanup(ns, [73; 32]),
        Err(OverlayError::Stale)
    ));
    assert!(matches!(
        db.observe_cleanup(0, incarnation),
        Err(OverlayError::Stale)
    ));
    assert!(matches!(
        db.observe_cleanup(ns, [0; 32]),
        Err(OverlayError::Stale)
    ));
    assert!(matches!(
        db.observe_cleanup(ns + 1, incarnation),
        Err(OverlayError::Stale)
    ));

    let source = db.acquire_base_source(route, 1).unwrap();
    db.close(route).unwrap();
    let before = db.resources(Some(route)).unwrap().counts;
    for _ in 0..2 {
        let sql = db.diagnostics();
        assert_eq!(
            db.observe_cleanup(ns, incarnation).unwrap(),
            CleanupState::Held
        );
        let work = db.diagnostics().since(&sql).total();
        assert_eq!(work.executions, 1);
        assert_eq!(work.rows_changed, 0);
        assert_eq!(work.direct_rows_changed, 0);
    }
    assert_eq!(db.resources(Some(route)).unwrap().counts, before);
    db.release_base_source(source).unwrap();
    assert_eq!(
        db.observe_cleanup(ns, incarnation).unwrap(),
        CleanupState::Queued
    );
    let mut gone = false;
    for _ in 0..40 {
        if db.reclaim_closed(0).unwrap().is_none() {
            break;
        }
        if db.observe_cleanup(ns, incarnation).unwrap() == CleanupState::Gone {
            gone = true;
            break;
        }
    }
    assert!(
        gone,
        "bounded explicit maintenance did not establish physical absence"
    );
    assert_eq!(
        db.observe_cleanup(ns, incarnation).unwrap(),
        CleanupState::Gone
    );
    let next = db.open_workspace([74; 32], [72; 32]).unwrap();
    assert!(next.namespace() > ns);
    assert_eq!(
        db.observe_cleanup(ns, incarnation).unwrap(),
        CleanupState::Gone
    );
    assert!(matches!(
        db.observe_cleanup(next.namespace(), incarnation),
        Err(OverlayError::Stale)
    ));
}

#[test]
fn fixed_point_work_is_equal_beside_two_unrelated_population_sizes() {
    let mut observed = Vec::new();
    for population in [64_u64, 256] {
        let temp = Temp::new();
        let db = temp.db();
        let incarnation = [81; 32];
        let route = db.open_workspace(incarnation, [82; 32]).unwrap();
        for n in 0..population {
            let mut identity = [83; 32];
            identity[..8].copy_from_slice(&n.to_be_bytes());
            db.open_workspace(identity, [82; 32]).unwrap();
        }
        let before = db.diagnostics();
        assert_eq!(
            db.observe_cleanup(route.namespace(), incarnation).unwrap(),
            CleanupState::Live
        );
        let work = db.diagnostics().since(&before);
        let row = work.statements[StatementKind::Workspace as usize];
        assert_eq!(row.executions, 1);
        assert_eq!(row.rows_returned, 1);
        assert_eq!(row.rows_changed + row.direct_rows_changed, 0);
        assert_eq!(row.sorts + row.autoindex_rows + row.reprepares, 0);
        println!(
            "R7_CLEANUP population={population} profile={:?} work={row:?} plans={:?}",
            db.profile(),
            db.explain_cleanup_observation().unwrap()
        );
        observed.push((
            row.executions,
            row.rows_returned,
            row.vm_steps,
            row.fullscan_steps,
        ));
    }
    assert_eq!(observed[0], observed[1]);
}
