//! Same actual authority rebind/native identities, retained failures and explicit idle drain.
#[cfg(any(target_os = "macos", target_os = "linux"))]
#[path = "support/construction_graph_fixture.rs"]
#[allow(dead_code)]
mod fixture;
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
    use layerfs_content::file::edit::{DraftCapacity, DraftState};
    use layerfs_content::filesystem::rows::BindingRows;
    use layerfs_content::filesystem::state::*;
    use layerfs_content::filesystem::validate::{solve_effective_graph, ValidationGraphWork};
    use layerfs_storage::construction_state::{
        DraftAdapter, ScratchAuthority, ScratchDisposition, ScratchSession,
    };
    fn open(
        authority: &ScratchAuthority,
        source: &impl BindingRows,
        selector: u8,
        bytes: u64,
    ) -> (ScratchSession, GraphConstructionScopes) {
        let session = authority
            .begin_graph([selector; 32], 0, 0, subject(source, false, bytes))
            .unwrap();
        let scopes = selected(&session);
        (session, scopes)
    }
    fn complete(session: &mut ScratchSession, scopes: &GraphConstructionScopes) {
        empty_sites(session, scopes);
        if scopes.graph().subject().mode() == GraphMode::Fresh {
            session.graph_root(scopes.graph()).unwrap();
            let root = session.graph_unexpanded(scopes.graph()).unwrap().unwrap();
            assert_eq!(root.key().serial(), 1);
            session.graph_expanded(scopes.graph(), &root).unwrap();
        }
        assert!(session.graph_unexpanded(scopes.graph()).unwrap().is_none());
        let adjacency = session.graph_seal(scopes.graph()).unwrap();
        if scopes.graph().subject().mode() == GraphMode::Update {
            if let Err(mapped) = solve_effective_graph(
                &mut session.adapter(),
                &adjacency,
                &mut ValidationGraphWork::default(),
            ) {
                panic!(
                    "Update SCC mapped={mapped:?}; original={:?}",
                    session.take_failure()
                );
            }
        }
        let proof = session.graph_finish(&adjacency).unwrap();
        session.graph_retire(&proof).unwrap();
        let roots = session.seal(scopes.roots()).unwrap();
        assert_eq!(roots.records(), 0);
        session.complete_phase(scopes.roots()).unwrap();
    }
    #[test]
    fn same_live_authority_fresh_token_source_full_header_and_stable_native_birth() {
        let temp = TempDir::new("scratch_pool_rebind");
        let authority = ScratchAuthority::new(temp.path(), 2).unwrap();
        with_source(0, false, |source_a, _| {
            let (mut first, scopes_a) = open(&authority, source_a, 0xa1, DEFAULT);
            let fd = first.native_descriptor_observation().unwrap();
            let token = first.selection().token();
            let old = scopes_a.graph().clone();
            let original = status(&authority, token);
            let path = original.path.clone();
            let file = original.file.unwrap();
            drop(original);
            let db = external(&path);
            let old_header: Vec<u8> = db
                .query_row("SELECT header FROM session_owner", [], |r| r.get(0))
                .unwrap();
            drop(db);
            complete(&mut first, &scopes_a);
            first.return_to_idle().unwrap();
            drop(first);
            assert_eq!(authority.pool_status().unwrap().idle, 1);
            assert_eq!(authority.reserved_bytes().unwrap(), DEFAULT);
            assert!(path.exists());
            with_source(0, false, |source_b, _| {
                let (mut second, scopes_b) = open(&authority, source_b, 0xa2, DEFAULT);
                assert_ne!(second.selection().token(), token);
                assert_eq!(second.native_descriptor_observation().unwrap(), fd);
                assert_ne!(
                    source_a.binding_source_id().unwrap(),
                    source_b.binding_source_id().unwrap()
                );
                let now = status(&authority, second.selection().token());
                assert_eq!(now.path, path);
                assert_eq!(now.file, Some(file));
                drop(now);
                let db = external(&path);
                let new_header: Vec<u8> = db
                    .query_row("SELECT header FROM session_owner", [], |r| r.get(0))
                    .unwrap();
                assert_ne!(new_header, old_header);
                assert_eq!(new_header.len(), 298);
                for table in ["graph_owner", "solver_owner"] {
                    let scope: Vec<u8> = db
                        .query_row(&format!("SELECT scope FROM {table} WHERE id=1"), [], |r| {
                            r.get(0)
                        })
                        .unwrap();
                    assert_eq!(scope.as_slice(), scopes_b.graph().as_bytes());
                    assert_ne!(scope.as_slice(), old.as_bytes());
                }
                assert_eq!(scalar(&db, "SELECT current_serial FROM solver_owner"), 0);
                assert_eq!(scalar(&db, "SELECT next_discovery FROM solver_owner"), 1);
                drop(db);
                assert!(second.graph_select(&old).is_err());
                second.graph_select(scopes_b.graph()).unwrap();
                complete(&mut second, &scopes_b);
                second.return_to_idle().unwrap();
            });
            let pool = authority.pool_status().unwrap();
            assert_eq!(
                (
                    pool.counters.fresh.succeeded,
                    pool.counters.rebind.succeeded,
                    pool.counters.returns.succeeded
                ),
                (1, 1, 2)
            );
            assert_eq!(pool.idle_bytes, DEFAULT);
            assert_eq!(authority.drain_idle().unwrap(), 1);
            assert!(!path.exists());
            assert_eq!(authority.reserved_bytes().unwrap(), 0);
            assert_eq!(authority.drain_idle().unwrap(), 0);
        });
    }
    #[test]
    fn actual_metadata_observer_withholds_idle_but_scope_copy_is_stale_after_rebind() {
        with_source(0, false, |source, _| {
            let temp = TempDir::new("scratch_pool_observer");
            let authority = ScratchAuthority::new(temp.path(), 1).unwrap();
            let (mut session, scopes) = open(&authority, source, 0xa3, DEFAULT);
            complete(&mut session, &scopes);
            let observer = status(&authority, session.selection().token())
                .graph_working
                .unwrap();
            assert!(session.return_to_idle().is_err());
            assert_eq!(authority.pool_status().unwrap().idle, 0);
            drop(observer);
            session.return_to_idle().unwrap();
            assert_eq!(authority.pool_status().unwrap().idle, 1);
            assert_eq!(authority.drain_idle().unwrap(), 1);
        });
    }
    #[test]
    fn exact16_48_classes_occupy_fixed_slots_and_refuse_different_class_before_birth() {
        with_source(0, false, |source, _| {
            let temp = TempDir::new("scratch_pool_classes");
            let authority = ScratchAuthority::new(temp.path(), 2).unwrap();
            let (mut a, sa) = open(&authority, source, 0xa4, DEFAULT);
            let (mut b, sb) = open(&authority, source, 0xa5, CONFIGURED);
            complete(&mut a, &sa);
            complete(&mut b, &sb);
            a.return_to_idle().unwrap();
            b.return_to_idle().unwrap();
            assert_eq!(authority.reserved_bytes().unwrap(), DEFAULT + CONFIGURED);
            let before = authority.pool_status().unwrap();
            assert_eq!(before.idle, 2);
            assert!(authority
                .begin_graph([0xa6; 32], 0, 0, subject(source, false, 32 * 1024 * 1024))
                .is_err());
            let after = authority.pool_status().unwrap();
            assert_eq!(
                after.counters.fresh.attempts,
                before.counters.fresh.attempts
            );
            assert_eq!(
                after.counters.class_refusals,
                before.counters.class_refusals + 1
            );
            let (mut reused, scope) = open(&authority, source, 0xa7, CONFIGURED);
            complete(&mut reused, &scope);
            reused.return_to_idle().unwrap();
            assert_eq!(
                authority.pool_status().unwrap().counters.rebind.succeeded,
                1
            );
            assert_eq!(authority.drain_idle().unwrap(), 2);
            assert_eq!(authority.reserved_bytes().unwrap(), 0);
        });
    }
    #[test]
    fn corrupt_idle_projection_is_retained_and_cannot_fall_back_to_empty_second_slot() {
        with_source(0, false, |source, _| {
            let temp = TempDir::new("scratch_pool_corrupt");
            let authority = ScratchAuthority::new(temp.path(), 2).unwrap();
            let (mut session, scopes) = open(&authority, source, 0xa8, DEFAULT);
            complete(&mut session, &scopes);
            let token = session.selection().token();
            let path = status(&authority, token).path;
            session.return_to_idle().unwrap();
            let db = external(&path);
            db.execute("UPDATE solver_owner SET next_discovery=2", [])
                .unwrap();
            drop(db);
            assert!(authority
                .begin_graph([0xa9; 32], 0, 0, subject(source, false, DEFAULT))
                .is_err());
            let pool = authority.pool_status().unwrap();
            assert_eq!(pool.counters.fresh.succeeded, 1);
            assert_eq!(
                (pool.counters.rebind.succeeded, pool.counters.rebind.failed),
                (0, 1)
            );
            assert_eq!(pool.idle, 0);
            assert_eq!(authority.drain_idle().unwrap(), 0);
            assert_eq!(authority.reserved_bytes().unwrap(), DEFAULT);
            let retained = authority.status().unwrap();
            assert_eq!(retained.len(), 1);
            assert!(retained[0].retained);
            let retained_token = retained[0].token;
            drop(retained);
            authority.release_retained(retained_token).unwrap();
            assert!(!path.exists());
        });
    }
    #[test]
    fn real_reader_commit_unknown_preserves_complete_old_proposed_context_and_s() {
        with_source(0, false, |source, _| {
            let temp = TempDir::new("scratch_pool_unknown");
            let authority = ScratchAuthority::new(temp.path(), 2).unwrap();
            let (mut session, scopes) = open(&authority, source, 0xaa, DEFAULT);
            complete(&mut session, &scopes);
            let token = session.selection().token();
            let path = status(&authority, token).path;
            session.return_to_idle().unwrap();
            let db = external(&path);
            db.execute_batch("BEGIN").unwrap();
            let old: Vec<u8> = db
                .query_row("SELECT header FROM session_owner", [], |r| r.get(0))
                .unwrap();
            assert_eq!(old.len(), 298);
            let error =
                match authority.begin_graph([0xab; 32], 0, 0, subject(source, false, DEFAULT)) {
                    Err(error) => error,
                    Ok(_) => panic!("reader must hold rebind COMMIT"),
                };
            assert!(
                matches!(error, layerfs_storage::StorageError::UnknownOutcome { .. }),
                "{error:?}"
            );
            let retained = authority.status().unwrap();
            assert_eq!(retained.len(), 1);
            assert_eq!(retained[0].disposition, ScratchDisposition::Unknown);
            assert!(retained[0].failure.as_ref().unwrap().contains("rebind"));
            assert!(retained[0]
                .failure
                .as_ref()
                .unwrap()
                .contains("proposed token="));
            let unknown = retained[0].token;
            drop(retained);
            assert_eq!(authority.reserved_bytes().unwrap(), DEFAULT);
            assert_eq!(authority.pool_status().unwrap().counters.fresh.succeeded, 1);
            assert_eq!(authority.drain_idle().unwrap(), 0);
            db.execute_batch("ROLLBACK").unwrap();
            drop(db);
            assert!(authority.release_retained(unknown).is_err());
            assert!(path.exists());
            std::mem::forget(temp);
        });
    }
    #[test]
    fn actual_finished_draft6_resets_before_idle_and_rebinds_full_scope_without_new_birth() {
        let temp = TempDir::new("scratch_pool_draft");
        let authority = ScratchAuthority::new(temp.path(), 1).unwrap();
        let capacity = DraftCapacity::default();
        let mut first = authority.begin_drafts([0xac; 32], capacity).unwrap();
        let old = first.draft_scope().unwrap();
        let token = first.selection().token();
        let first_info = status(&authority, token);
        let path = first_info.path.clone();
        let file = first_info.file;
        drop(first_info);
        DraftAdapter::new(&mut first).unwrap().finish().unwrap();
        first.return_to_idle().unwrap();
        assert_eq!(authority.pool_status().unwrap().idle, 1);
        let db = external(&path);
        assert_eq!(scalar(&db, "SELECT stage FROM draft_owner"), 0);
        assert_eq!(scalar(&db, "SELECT next_job FROM draft_owner"), 1);
        drop(db);
        let mut second = authority.begin_drafts([0xad; 32], capacity).unwrap();
        let new = second.draft_scope().unwrap();
        assert_ne!(old, new);
        assert_ne!(second.selection().token(), token);
        let second_info = status(&authority, second.selection().token());
        assert_eq!(second_info.path, path);
        assert_eq!(second_info.file, file);
        drop(second_info);
        DraftAdapter::new(&mut second).unwrap().finish().unwrap();
        second.return_to_idle().unwrap();
        assert_eq!(authority.pool_status().unwrap().counters.fresh.succeeded, 1);
        assert_eq!(
            authority.pool_status().unwrap().counters.rebind.succeeded,
            1
        );
        assert_eq!(authority.drain_idle().unwrap(), 1);
        assert_eq!(authority.reserved_bytes().unwrap(), 0);
    }
}
