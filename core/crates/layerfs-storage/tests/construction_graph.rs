//! Actual private-v4 owner/SQLite/phase proofs; no benchmark or speed admission.
//! Finite symbolic namespace/Base fixtures qualify private ports/resources only;
//! actual canonical C1/Server namespace predicates have separate owning proofs.

#[cfg(any(target_os = "macos", target_os = "linux"))]
#[path = "support/construction_graph_fixture.rs"]
mod fixture;
#[cfg(any(target_os = "macos", target_os = "linux"))]
#[path = "support/construction_graph_oracle.rs"]
mod graph_oracle;
#[cfg(any(target_os = "macos", target_os = "linux"))]
#[path = "support/construction_graph_process.rs"]
mod process;
#[cfg(any(target_os = "macos", target_os = "linux"))]
#[path = "support/construction_graph_resources.rs"]
mod resources;
#[cfg(any(target_os = "macos", target_os = "linux"))]
#[path = "support/construction_oracle.rs"]
#[allow(dead_code)]
mod roots_oracle;
#[cfg(any(target_os = "macos", target_os = "linux"))]
mod support;
#[cfg(any(target_os = "macos", target_os = "linux"))]
#[path = "support/construction_graph_unknown.rs"]
mod unknown;

#[cfg(any(target_os = "macos", target_os = "linux"))]
mod unix {
    use super::fixture::*;
    use super::{graph_oracle, roots_oracle, support::TempDir};
    use layerfs_content::filesystem::state::*;
    use layerfs_content::filesystem::validate::{solve_effective_graph, ValidationGraphWork};
    use layerfs_content::ContentError;
    use layerfs_storage::construction_state::{ScratchAuthority, ScratchDisposition};
    use layerfs_storage::StorageError;

    fn raw_scope(scopes: &GraphConstructionScopes) -> [u8; 188] {
        let selected = scopes.graph().nodes().selection();
        graph_oracle::scope(
            *selected.selector(),
            selected.token(),
            *selected.owner_binding().unwrap(),
            graph_oracle::subject(
                scopes.graph().subject().source_id().as_bytes(),
                scopes.graph().subject().base().is_some(),
                1,
                scopes.graph().capacity().scratch_bytes(),
            ),
        )
    }
    fn solve(
        session: &mut layerfs_storage::construction_state::ScratchSession,
        seal: &GraphAdjacencySeal,
    ) -> ValidationGraphWork {
        let mut work = ValidationGraphWork::default();
        solve_effective_graph(&mut session.adapter(), seal, &mut work).unwrap();
        work
    }

    #[test]
    fn literal_header_schema_selected_subject_and_empty_graph_root_phase() {
        with_source(0, true, |source, _| {
            let temp = TempDir::new("graph_header");
            let authority = ScratchAuthority::new(temp.path(), 1).unwrap();
            let (mut session, scopes) = begin(&authority, source, true, DEFAULT, 2, 0);
            let owner = status(&authority, session.selection().token());
            assert_header(temp.path(), &owner, scopes.graph());
            assert_eq!(scopes.graph().as_bytes(), raw_scope(&scopes));
            assert_eq!(session.profile().unwrap().version, 4);
            assert_eq!(session.profile().unwrap().max_page_count, 4096);
            session.graph_select(scopes.graph()).unwrap();
            empty_sites(&mut session, &scopes);
            assert_eq!(
                session.graph_capacity(scopes.graph()).unwrap(),
                GraphCapacity::default()
            );
            assert!(session.graph_unexpanded(scopes.graph()).unwrap().is_none());
            let adjacency = session.graph_seal(scopes.graph()).unwrap();
            assert_eq!(
                adjacency.encode(),
                graph_oracle::adjacency(&raw_scope(&scopes), &[], &[])
            );
            solve(&mut session, &adjacency);
            let proof = session.graph_finish(&adjacency).unwrap();
            assert_eq!(
                proof.encode(),
                graph_oracle::proof(&adjacency.encode(), &[])
            );
            assert!(session
                .graph_node_page(&adjacency, None, GraphPageLimit::new(128, 318).unwrap())
                .unwrap()
                .eof());
            assert!(session
                .graph_edge_page(&adjacency, 1, None, GraphPageLimit::new(128, 318).unwrap())
                .unwrap()
                .eof());
            assert!(session
                .graph_proof_page(&proof, None, GraphPageLimit::new(128, 350).unwrap())
                .unwrap()
                .eof());
            session.graph_retire(&proof).unwrap();
            assert_eq!(session.capacity(scopes.roots()).unwrap().records(), 2);
            session
                .append(
                    scopes.roots(),
                    &[root(scopes.roots(), 1), root(scopes.roots(), 2)],
                )
                .unwrap();
            let seal = session.seal(scopes.roots()).unwrap();
            let mut expected = roots_oracle::Transcript::new(roots_oracle::scope(
                owner.selector,
                owner.token,
                *session.selection().owner_binding().unwrap(),
                3,
            ));
            expected.append(1);
            expected.append(2);
            assert_eq!(seal.encode(), expected.seal());
            session.complete_phase(scopes.roots()).unwrap();
            session.release().unwrap();
            assert_eq!(authority.reserved_bytes().unwrap(), 0);
        });
    }

    #[test]
    fn independent_multiarc_bytes_seals_pages_live_solver_and_roots() {
        with_source(3, true, |source, _| {
            let temp = TempDir::new("graph_dag");
            let authority = ScratchAuthority::new(temp.path(), 1).unwrap();
            let (mut session, scopes) = begin(&authority, source, true, DEFAULT, 3, 3);
            empty_sites(&mut session, &scopes);
            let seal = graph(&mut session, &scopes, &[1], |serial| {
                if serial == 1 {
                    vec![2, 3, 2]
                } else {
                    vec![]
                }
            });
            let raw = raw_scope(&scopes);
            let nodes = [
                graph_oracle::node(&raw, 1, (3, 0, 0, 0, 0, 0)),
                graph_oracle::node(&raw, 2, (2, 0, 0, 0, 0, 2)),
                graph_oracle::node(&raw, 3, (2, 0, 0, 0, 0, 1)),
            ];
            let edges = [
                graph_oracle::edge(&raw, 1, 2, 2),
                graph_oracle::edge(&raw, 1, 3, 1),
            ];
            assert_eq!(seal.encode(), graph_oracle::adjacency(&raw, &nodes, &edges));
            let mut cursor = GraphNodeCursor::new(seal.clone());
            let limit = GraphPageLimit::new(1, 378).unwrap();
            while !cursor.finished() {
                let page = session
                    .graph_node_page(&seal, cursor.after(), limit)
                    .unwrap();
                assert_eq!(page.records().len(), 1);
                cursor.accept(&page, limit).unwrap();
            }
            assert_eq!(cursor.records(), 3);
            let mut edge_cursor = GraphEdgeCursor::new(seal.clone(), 1).unwrap();
            let limit = GraphPageLimit::new(1, 361).unwrap();
            while !edge_cursor.finished() {
                let page = session
                    .graph_edge_page(&seal, 1, edge_cursor.after(), limit)
                    .unwrap();
                assert_eq!(page.maximum(), Some(3));
                edge_cursor.accept(&page, limit).unwrap();
            }
            let work = solve(&mut session, &seal);
            assert_eq!(
                (work.solver_entered, work.edge_steps, work.popped_members),
                (3, 2, 3)
            );
            let page = session
                .graph_node_page(&seal, None, GraphPageLimit::default())
                .unwrap();
            assert_eq!(
                page.records()
                    .iter()
                    .map(|node| node.encode())
                    .collect::<Vec<_>>(),
                nodes
            );
            let full = [
                graph_oracle::node(&raw, 1, (83, 1, 1, 0, 3, 0)),
                graph_oracle::node(&raw, 2, (82, 2, 2, 1, 0, 2)),
                graph_oracle::node(&raw, 3, (82, 3, 3, 1, 0, 1)),
            ];
            let proof = session.graph_finish(&seal).unwrap();
            assert_eq!(proof.encode(), graph_oracle::proof(&seal.encode(), &full));
            let mut cursor = GraphProofCursor::new(proof.clone());
            while !cursor.finished() {
                let page = session
                    .graph_proof_page(&proof, cursor.after(), GraphPageLimit::default())
                    .unwrap();
                cursor.accept(&page, GraphPageLimit::default()).unwrap();
            }
            session.graph_retire(&proof).unwrap();
            session
                .append(scopes.roots(), &[root(scopes.roots(), 1)])
                .unwrap();
            session.seal(scopes.roots()).unwrap();
            session.release().unwrap();
        });
    }

    #[test]
    fn cyclic_seed_last_pop_returns_members_and_known_rejection_without_provider_error() {
        with_source(1, true, |source, _| {
            let temp = TempDir::new("graph_cycle");
            let authority = ScratchAuthority::new(temp.path(), 1).unwrap();
            let (mut session, scopes) = begin(&authority, source, true, DEFAULT, 1, 1);
            empty_sites(&mut session, &scopes);
            let seal = graph(&mut session, &scopes, &[1], |_| vec![1]);
            session.graph_begin_scc(&seal).unwrap();
            let key = GraphNodeKey::new(scopes.graph(), 1).unwrap();
            let before = session.graph_node(&seal, key).unwrap().unwrap();
            let mut active = before.enter(scopes.graph(), 1, 0).unwrap();
            session
                .graph_cas(
                    &seal,
                    &[GraphMutation::EnterRoot {
                        before,
                        after: active,
                    }],
                )
                .unwrap();
            let edge = session
                .graph_edge_page(&seal, 1, None, GraphPageLimit::default())
                .unwrap()
                .records()[0];
            let advanced = active.advance(scopes.graph(), 1, 1).unwrap();
            session
                .graph_cas(
                    &seal,
                    &[GraphMutation::Advance {
                        before: active,
                        after: advanced,
                        edge,
                    }],
                )
                .unwrap();
            active = advanced;
            let finished = active.finish(scopes.graph()).unwrap();
            session
                .graph_cas(
                    &seal,
                    &[GraphMutation::Finish {
                        before: active,
                        after: finished,
                    }],
                )
                .unwrap();
            let ack = session
                .graph_pop(&seal, &finished, GraphMutationLimit::default())
                .unwrap();
            assert_eq!(ack.disposition(), GraphPopDisposition::RejectedCycle);
            assert_eq!(
                (ack.popped(), ack.any_seed(), ack.singleton_self_loop()),
                (1, true, true)
            );
            assert_eq!(ack.members().len(), 1);
            assert_eq!(
                ack.members()[0].encode(),
                graph_oracle::node(&raw_scope(&scopes), 1, (115, 1, 1, 0, 1, 1))
            );
            assert!(session.take_failure().is_none());
            assert!(!session.is_quarantined());
            let owner = status(&authority, session.selection().token());
            assert_eq!(owner.disposition, ScratchDisposition::Failed);
            assert_eq!(
                scalar(&external(&owner.path), "SELECT stage FROM graph_owner"),
                8
            );
            assert_eq!(
                scalar(
                    &external(&owner.path),
                    "SELECT COUNT(*) FROM directory_roots"
                ),
                0
            );
            assert_eq!(authority.reserved_bytes().unwrap(), DEFAULT);
            assert!(session.capacity(scopes.roots()).is_err());
            session.release().unwrap();
        });
    }

    #[test]
    fn old_nonseed_descendant_cycle_keeps_seed_acyclic_and_closes_exact_proof() {
        with_source(3, true, |source, _| {
            let temp = TempDir::new("graph_old_cycle");
            let authority = ScratchAuthority::new(temp.path(), 1).unwrap();
            let (mut session, scopes) = begin(&authority, source, true, DEFAULT, 1, 3);
            empty_sites(&mut session, &scopes);
            let seal = graph(&mut session, &scopes, &[1], |serial| match serial {
                1 => vec![2],
                2 => vec![3],
                3 => vec![2],
                _ => vec![],
            });
            let work = solve(&mut session, &seal);
            assert_eq!((work.solver_entered, work.popped_members), (3, 3));
            let proof = session.graph_finish(&seal).unwrap();
            let page = session
                .graph_proof_page(&proof, None, GraphPageLimit::default())
                .unwrap();
            assert_eq!(
                page.records()
                    .iter()
                    .map(|node| node.lowlink())
                    .collect::<Vec<_>>(),
                vec![1, 2, 2]
            );
            assert!(page.records().iter().all(|node| node.completed()));
            session.graph_retire(&proof).unwrap();
            session.release().unwrap();
        });
    }

    #[test]
    fn fresh_root_only_reachability_is_not_scc_done() {
        with_source(2, false, |source, _| {
            let temp = TempDir::new("graph_fresh");
            let authority = ScratchAuthority::new(temp.path(), 1).unwrap();
            let (mut session, scopes) = begin(&authority, source, false, DEFAULT, 1, 2);
            empty_sites(&mut session, &scopes);
            session.graph_root(scopes.graph()).unwrap();
            while let Some(mut parent) = session.graph_unexpanded(scopes.graph()).unwrap() {
                if parent.key().serial() == 1 {
                    let keys = [GraphNodeKey::new(scopes.graph(), 2).unwrap()];
                    parent = session
                        .graph_append(scopes.graph(), &parent, &keys)
                        .unwrap()
                        .parent()
                        .unwrap();
                }
                session.graph_expanded(scopes.graph(), &parent).unwrap();
            }
            let seal = session.graph_seal(scopes.graph()).unwrap();
            let page = session
                .graph_node_page(&seal, None, GraphPageLimit::default())
                .unwrap();
            assert!(page
                .records()
                .iter()
                .all(|node| node.root_reached() && !node.completed() && node.discovery() == 0));
            assert!(session
                .graph_node(&seal, GraphNodeKey::new(scopes.graph(), 3).unwrap())
                .unwrap()
                .is_none());
            let proof = session.graph_finish(&seal).unwrap();
            session.graph_retire(&proof).unwrap();
            session.release().unwrap();
        });
    }

    #[test]
    fn pure_foreign_subject_selection_under_exclusive_lock_preserves_valid_site_owner() {
        with_source(0, true, |source, _| {
            let temp = TempDir::new("graph_selection");
            let authority = ScratchAuthority::new(temp.path(), 1).unwrap();
            let (mut session, scopes) = begin(&authority, source, true, DEFAULT, 1, 0);
            let owner = status(&authority, session.selection().token());
            let before = std::fs::read(&owner.path).unwrap();
            let connection = external(&owner.path);
            connection.execute_batch("BEGIN EXCLUSIVE").unwrap();
            let foreign = GraphConstructionScopes::new(
                session.selection().clone(),
                subject(source, true, CONFIGURED),
            )
            .unwrap();
            assert!(matches!(
                session.adapter().graph_select(foreign.graph()),
                Err(ContentError::InvalidOrderingRecord("graph foreign scope"))
            ));
            session.graph_select(scopes.graph()).unwrap();
            assert!(session.take_failure().is_none());
            let after = status(&authority, owner.token);
            assert_eq!(after.disposition, ScratchDisposition::Open);
            assert_eq!(after.failure, None);
            assert_eq!(std::fs::read(&owner.path).unwrap(), before);
            connection.execute_batch("ROLLBACK").unwrap();
            drop(connection);
            empty_sites(&mut session, &scopes);
            let seal = graph(&mut session, &scopes, &[], |_| vec![]);
            solve(&mut session, &seal);
            let proof = session.graph_finish(&seal).unwrap();
            session.graph_retire(&proof).unwrap();
            session.release().unwrap();
        });
    }

    #[test]
    fn mixed_legacy_and_configured_owners_sum_and_refund_their_captured_budgets() {
        with_source(0, true, |source, _| {
            let temp = TempDir::new("graph_mixed");
            let authority = ScratchAuthority::new(temp.path(), 2).unwrap();
            let mut legacy = authority.begin([0x65; 32], 0).unwrap();
            let (configured, _) = begin(&authority, source, true, CONFIGURED, 0, 0);
            let token = configured.selection().token();
            assert_eq!(authority.reserved_bytes().unwrap(), DEFAULT + CONFIGURED);
            assert_eq!(configured.profile().unwrap().max_page_count, 12288);
            assert_eq!(status(&authority, token).allocated_bytes, Some(CONFIGURED));
            drop(configured);
            let retained = status(&authority, token);
            assert!(retained.retained);
            assert_eq!(retained.reserved_bytes, CONFIGURED);
            authority.release_retained(token).unwrap();
            assert_eq!(authority.reserved_bytes().unwrap(), DEFAULT);
            legacy.release().unwrap();
            assert_eq!(authority.reserved_bytes().unwrap(), 0);
        });
    }

    #[test]
    fn overdeclared_source_shape_refuses_before_slot_file_and_sql() {
        with_source(0, true, |source, _| {
            let temp = TempDir::new("graph_declaration");
            let authority = ScratchAuthority::new(temp.path(), 1).unwrap();
            let outcome =
                authority.begin_graph([0x66; 32], 1, ROWS + 1, subject(source, true, CONFIGURED));
            assert!(matches!(
                outcome,
                Err(StorageError::CapacityExceeded {
                    what: "construction scratch phased declared rows",
                    ..
                })
            ));
            assert!(authority.status().unwrap().is_empty());
            assert_eq!(authority.reserved_bytes().unwrap(), 0);
            assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 0);
        });
    }

    #[test]
    fn zero_progress_page_capacity_refuses_before_query_under_real_exclusive_owner() {
        with_source(1, true, |source, _| {
            let temp = TempDir::new("graph_page_refusal");
            let authority = ScratchAuthority::new(temp.path(), 1).unwrap();
            let (mut session, scopes) = begin(&authority, source, true, DEFAULT, 1, 1);
            empty_sites(&mut session, &scopes);
            let seal = graph(&mut session, &scopes, &[1], |_| vec![]);
            let owner = status(&authority, session.selection().token());
            let connection = external(&owner.path);
            connection.execute_batch("BEGIN EXCLUSIVE").unwrap();
            let outcome =
                session.graph_node_page(&seal, None, GraphPageLimit::new(128, 318).unwrap());
            assert!(matches!(
                outcome,
                Err(StorageError::Content(
                    ContentError::BoundedCapacityExceeded {
                        what: "graph.page_bytes",
                        ..
                    }
                ))
            ));
            assert_eq!(scalar(&connection, "SELECT COUNT(*) FROM graph_nodes"), 1);
            assert_eq!(scalar(&connection, "SELECT stage FROM graph_owner"), 3);
            assert_eq!(
                scalar(&connection, "SELECT COUNT(*) FROM directory_roots"),
                0
            );
            assert!(!status(&authority, owner.token)
                .failure
                .unwrap()
                .contains("graph attempt"));
            connection.execute_batch("ROLLBACK").unwrap();
            drop(connection);
            session.release().unwrap();
        });
    }

    #[test]
    fn selected_redundant_projection_corruption_refuses_no_current_batch_effects() {
        with_source(3, true, |source, _| {
            let temp = TempDir::new("graph_corrupt");
            let authority = ScratchAuthority::new(temp.path(), 1).unwrap();
            let (mut session, scopes) = begin(&authority, source, true, DEFAULT, 1, 3);
            empty_sites(&mut session, &scopes);
            seeds(&mut session, &scopes, [1, 3]);
            let owner = status(&authority, session.selection().token());
            let connection = external(&owner.path);
            assert_eq!(
                connection
                    .execute(
                        "UPDATE graph_nodes SET flags=32 WHERE key=?1",
                        [GraphNodeKey::new(scopes.graph(), 3)
                            .unwrap()
                            .as_bytes()
                            .as_slice()]
                    )
                    .unwrap(),
                1
            );
            drop(connection);
            let outcome = session.graph_seed_batch(
                scopes.graph(),
                &[
                    GraphNodeKey::new(scopes.graph(), 1).unwrap(),
                    GraphNodeKey::new(scopes.graph(), 2).unwrap(),
                    GraphNodeKey::new(scopes.graph(), 3).unwrap(),
                ],
            );
            assert!(matches!(
                outcome,
                Err(StorageError::Integrity(
                    "construction scratch graph redundant node"
                ))
            ));
            assert_eq!(
                scalar(&external(&owner.path), "SELECT COUNT(*) FROM graph_nodes"),
                2
            );
            assert_eq!(
                scalar(
                    &external(&owner.path),
                    "SELECT COUNT(*) FROM directory_roots"
                ),
                0
            );
            assert!(!session.is_quarantined());
            session.release().unwrap();
        });
    }

    #[test]
    fn known_partial_retirement_keeps_progress_and_denies_roots_without_retry() {
        with_source(129, true, |source, _| {
            let temp = TempDir::new("graph_partial_retire");
            let authority = ScratchAuthority::new(temp.path(), 1).unwrap();
            let (mut session, scopes) = begin(&authority, source, true, DEFAULT, 1, 129);
            empty_sites(&mut session, &scopes);
            seeds(&mut session, &scopes, 1..=129);
            while let Some(parent) = session.graph_unexpanded(scopes.graph()).unwrap() {
                session.graph_expanded(scopes.graph(), &parent).unwrap();
            }
            let seal = session.graph_seal(scopes.graph()).unwrap();
            solve(&mut session, &seal);
            let proof = session.graph_finish(&seal).unwrap();
            let owner = status(&authority, session.selection().token());
            let connection = external(&owner.path);
            let key = GraphNodeKey::new(scopes.graph(), 129).unwrap();
            let mut value: Vec<u8> = connection
                .query_row(
                    "SELECT value FROM graph_nodes WHERE key=?1",
                    [key.as_bytes().as_slice()],
                    |row| row.get(0),
                )
                .unwrap();
            value[0] |= 128;
            assert_eq!(
                connection
                    .execute(
                        "UPDATE graph_nodes SET value=?1 WHERE key=?2",
                        rusqlite::params![value, key.as_bytes().as_slice()]
                    )
                    .unwrap(),
                1
            );
            drop(connection);
            assert!(session.graph_retire(&proof).is_err());
            let connection = external(&owner.path);
            assert_eq!(scalar(&connection, "SELECT COUNT(*) FROM graph_nodes"), 1);
            assert_eq!(scalar(&connection, "SELECT stage FROM graph_owner"), 6);
            assert_eq!(
                scalar(&connection, "SELECT remaining_nodes FROM graph_owner"),
                1
            );
            assert_eq!(
                scalar(&connection, "SELECT COUNT(*) FROM directory_roots"),
                0
            );
            drop(connection);
            assert_eq!(authority.reserved_bytes().unwrap(), DEFAULT);
            assert!(!session.is_quarantined());
            assert!(session.capacity(scopes.roots()).is_err());
            session.release().unwrap();
        });
    }
}
