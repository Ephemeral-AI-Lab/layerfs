//! Profile4 exact root retirement/reset through real current native owners.
//! Private-format/count proofs do not qualify whole-host memory or speed.

#[cfg(any(target_os = "macos", target_os = "linux"))]
#[path = "support/construction_graph_fixture.rs"]
#[allow(dead_code)]
mod fixture;
#[cfg(any(target_os = "macos", target_os = "linux"))]
#[cfg(any(target_os = "macos", target_os = "linux"))]
#[path = "support/construction_graph_oracle.rs"]
#[allow(dead_code)]
mod graph_oracle;
#[cfg(any(target_os = "macos", target_os = "linux"))]
#[path = "support/construction_oracle.rs"]
#[allow(dead_code)]
mod roots_oracle;
#[cfg(any(target_os = "macos", target_os = "linux"))]
mod support;

#[cfg(any(target_os = "macos", target_os = "linux"))]
mod unix {
    use super::{fixture::*, support::TempDir};
    use layerfs_content::filesystem::state::*;
    use layerfs_content::filesystem::validate::{solve_effective_graph, ValidationGraphWork};
    use layerfs_storage::construction_state::{
        RootRetirementStage, ScratchAuthority, ScratchSession,
    };

    fn roots(
        session: &mut ScratchSession,
        scopes: &GraphConstructionScopes,
        count: u64,
    ) -> StateSeal {
        empty_sites(session, scopes);
        assert!(session.graph_unexpanded(scopes.graph()).unwrap().is_none());
        let adjacency = session.graph_seal(scopes.graph()).unwrap();
        solve_effective_graph(
            &mut session.adapter(),
            &adjacency,
            &mut ValidationGraphWork::default(),
        )
        .unwrap();
        let proof = session.graph_finish(&adjacency).unwrap();
        session.graph_retire(&proof).unwrap();
        let mut expected = StateLedger::new(scopes.roots().clone());
        let mut batch = Vec::with_capacity(128);
        for serial in 1..=count {
            batch.push(root(scopes.roots(), serial));
            if batch.len() == 128 || serial == count {
                expected.acknowledge(&batch).unwrap();
                session.append(scopes.roots(), &batch).unwrap();
                batch.clear();
            }
        }
        let actual = session.seal(scopes.roots()).unwrap();
        assert_eq!(actual, expected.seal());
        actual
    }

    #[test]
    fn exact_boundary_and_maximum_windows_reset_all_fixed_rows_without_refund() {
        for count in [0, 1, 128, 129, 65536] {
            with_source(0, true, |source, _| {
                let temp = TempDir::new("roots_known_reset");
                let authority = ScratchAuthority::new(temp.path(), 1).unwrap();
                let (mut session, scopes) = begin(&authority, source, true, DEFAULT, count, 0);
                let seal = roots(&mut session, &scopes, count);
                let owner = status(&authority, session.selection().token());
                let mut retired = 0;
                loop {
                    let progress = session.retire_roots_window(&seal).unwrap();
                    assert!(progress.retired_records - retired <= 128);
                    assert_eq!(progress.retired_bytes, progress.retired_records * 63);
                    assert_eq!(progress.remaining_records, count - progress.retired_records);
                    retired = progress.retired_records;
                    assert!(session.selection().token() != 0);
                    let native = status(&authority, owner.token);
                    assert_eq!(native.reserved_bytes, DEFAULT);
                    if progress.remaining_records == 0 {
                        break;
                    }
                }
                assert_eq!(retired, count);
                session.reset_completed_roots(&seal).unwrap();
                let clean = status(&authority, owner.token);
                assert!(clean.known_clean);
                assert_eq!(
                    clean.root_retirement.as_ref().unwrap().stage,
                    RootRetirementStage::Clean
                );
                assert_eq!(clean.root_retirement.as_ref().unwrap().seal, seal);
                assert_eq!(clean.file, owner.file);
                assert_eq!(clean.allocated_bytes, Some(DEFAULT));
                assert_eq!(authority.reserved_bytes().unwrap(), DEFAULT);
                assert_eq!(session.profile().unwrap().version, 4);
                let db = external(&owner.path);
                for table in [
                    "directory_roots",
                    "binding_sites",
                    "graph_nodes",
                    "graph_edges",
                ] {
                    assert_eq!(scalar(&db, &format!("SELECT COUNT(*) FROM {table}")), 0);
                }
                assert_eq!(
                    scalar(&db, "SELECT records+record_bytes+sealed FROM session_owner"),
                    0
                );
                assert_eq!(
                    scalar(&db, "SELECT stage+records+remaining FROM site_owner"),
                    0
                );
                assert_eq!(scalar(&db,"SELECT stage+nodes+edges+remaining_nodes+remaining_edges+seed_count FROM graph_owner"),0);
                assert_eq!(scalar(&db, "SELECT next_discovery FROM solver_owner"), 1);
                drop(db);
                session.release().unwrap();
                assert_eq!(authority.reserved_bytes().unwrap(), 0);
            });
        }
    }

    #[test]
    fn current_completion_route_performs_retirement_reset_before_return() {
        with_source(0, true, |source, _| {
            let temp = TempDir::new("roots_completion");
            let authority = ScratchAuthority::new(temp.path(), 1).unwrap();
            let (mut session, scopes) = begin(&authority, source, true, DEFAULT, 129, 0);
            let seal = roots(&mut session, &scopes, 129);
            let owner = status(&authority, session.selection().token());
            session.complete_phase(scopes.roots()).unwrap();
            let clean = status(&authority, owner.token);
            assert!(clean.known_clean);
            assert_eq!(clean.root_retirement.unwrap().retired_records, 129);
            assert!(session
                .get(&seal, StateKey::directory_root(scopes.roots(), 1).unwrap())
                .is_err());
            assert_eq!(
                scalar(
                    &external(&owner.path),
                    "SELECT COUNT(*) FROM directory_roots"
                ),
                0
            );
            session.release().unwrap();
        });
    }

    #[test]
    fn corrupted_terminal_row_preserves_partial_continuation_and_refuses_reset() {
        with_source(0, true, |source, _| {
            let temp = TempDir::new("roots_corrupt_retirement");
            let authority = ScratchAuthority::new(temp.path(), 1).unwrap();
            let (mut session, scopes) = begin(&authority, source, true, DEFAULT, 129, 0);
            let seal = roots(&mut session, &scopes, 129);
            let owner = status(&authority, session.selection().token());
            let first = session.retire_roots_window(&seal).unwrap();
            assert_eq!((first.retired_records, first.remaining_records), (128, 1));
            let db = external(&owner.path);
            db.execute(
                "UPDATE directory_roots SET root=zeroblob(32) WHERE ordinal=129",
                [],
            )
            .unwrap();
            assert!(session.retire_roots_window(&seal).is_err());
            let failed = status(&authority, owner.token);
            assert!(!failed.known_clean);
            assert_eq!(failed.root_retirement.unwrap().retired_records, 128);
            assert_eq!(scalar(&db, "SELECT COUNT(*) FROM directory_roots"), 1);
            assert!(session.reset_completed_roots(&seal).is_err());
            drop(db);
            session.release().unwrap();
        });
    }

    #[test]
    fn real_shared_reader_commit_unknown_retains_exact_window_and_denies_reset_refund() {
        with_source(0, true, |source, _| {
            let temp = TempDir::new("roots_unknown_retirement");
            let authority = ScratchAuthority::new(temp.path(), 1).unwrap();
            let (mut session, scopes) = begin(&authority, source, true, DEFAULT, 129, 0);
            let seal = roots(&mut session, &scopes, 129);
            let owner = status(&authority, session.selection().token());
            let db = external(&owner.path);
            db.execute_batch("BEGIN").unwrap();
            assert_eq!(scalar(&db, "SELECT COUNT(*) FROM directory_roots"), 129);
            assert!(session.retire_roots_window(&seal).is_err());
            assert!(session.is_quarantined());
            let unknown = status(&authority, owner.token);
            let progress = unknown.root_retirement.as_ref().unwrap();
            assert_eq!(progress.retired_records, 0);
            assert_eq!(progress.proposed_records, Some(128));
            assert_eq!(progress.pending_records, 128);
            assert_eq!(progress.proposed_after.unwrap().serial(), 128);
            assert!(unknown
                .failure
                .as_ref()
                .unwrap()
                .contains("pending exact rows"));
            db.execute_batch("ROLLBACK").unwrap();
            drop(db);
            let charged = unknown.graph_working.as_ref().unwrap().reserved_bytes();
            assert!(session.reset_completed_roots(&seal).is_err());
            assert!(session.release().is_err());
            assert_eq!(authority.reserved_bytes().unwrap(), DEFAULT);
            assert_eq!(
                unknown.graph_working.as_ref().unwrap().reserved_bytes(),
                charged
            );
            drop(session);
            assert!(status(&authority, owner.token).retained);
            // Intentional unresolved custody remains until process teardown.
            // The external fixture must not unlink a live quarantined owner.
            eprintln!("root retirement Unknown retained at {}; class={DEFAULT}, graphworking={charged}; no refund/adoption/clean claim",temp.path().display());
            std::mem::forget(authority);
            std::mem::forget(temp);
        });
    }

    #[test]
    fn fixed_reset_commit_unknown_keeps_terminal_seal_and_original_native_credit() {
        with_source(0, true, |source, _| {
            let temp = TempDir::new("roots_unknown_reset");
            let authority = ScratchAuthority::new(temp.path(), 1).unwrap();
            let (mut session, scopes) = begin(&authority, source, true, DEFAULT, 1, 0);
            let seal = roots(&mut session, &scopes, 1);
            let owner = status(&authority, session.selection().token());
            assert_eq!(
                session
                    .retire_roots_window(&seal)
                    .unwrap()
                    .remaining_records,
                0
            );
            let db = external(&owner.path);
            db.execute_batch("BEGIN").unwrap();
            assert_eq!(scalar(&db, "SELECT sealed FROM session_owner"), 1);
            assert!(session.reset_completed_roots(&seal).is_err());
            assert!(session.is_quarantined());
            let unknown = status(&authority, owner.token);
            let progress = unknown.root_retirement.unwrap();
            assert_eq!(progress.stage, RootRetirementStage::Resetting);
            assert_eq!(progress.seal, seal);
            assert_eq!(progress.retired_records, 1);
            assert!(!unknown.known_clean);
            db.execute_batch("ROLLBACK").unwrap();
            drop(db);
            assert!(session.release().is_err());
            assert_eq!(authority.reserved_bytes().unwrap(), DEFAULT);
            drop(session);
            assert!(status(&authority, owner.token).retained);
            eprintln!("fixed owner reset Unknown retained at {}; exact terminalseal retained; no retry/rebind/refund",temp.path().display());
            std::mem::forget(authority);
            std::mem::forget(temp);
        });
    }

    #[test]
    fn original_profile_one_completion_retains_its_explicit_logical_semantics() {
        let temp = TempDir::new("roots_legacy_complete");
        let authority = ScratchAuthority::new(temp.path(), 1).unwrap();
        let mut session = authority.begin([0x65; 32], 1).unwrap();
        let scope =
            StateScope::new(session.selection().clone(), 1, StateTable::DirectoryRoots).unwrap();
        session.append(&scope, &[root(&scope, 1)]).unwrap();
        session.seal(&scope).unwrap();
        let owner = status(&authority, session.selection().token());
        session.complete_phase(&scope).unwrap();
        let ended = status(&authority, owner.token);
        assert!(!ended.known_clean);
        assert!(ended.root_retirement.is_none());
        assert_eq!(
            scalar(
                &external(&owner.path),
                "SELECT COUNT(*) FROM directory_roots"
            ),
            1
        );
        session.release().unwrap();
    }
}
