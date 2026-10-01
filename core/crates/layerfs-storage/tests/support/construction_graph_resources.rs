//! Prospective native-format maximum shapes; actual outcomes belong to root's run.
//! These are independent count/disk/provider proofs, not C1 work or speed admission.

use super::fixture::*;
use super::{roots_oracle, support::TempDir};
use layerfs_content::filesystem::state::*;
use layerfs_content::filesystem::validate::{solve_effective_graph, ValidationGraphWork};
use layerfs_content::ContentError;
use layerfs_storage::construction_state::ScratchAuthority;
use layerfs_storage::StorageError;
use rusqlite::OptionalExtension;

fn count(path: &std::path::Path, table: &str) -> u64 {
    scalar(&external(path),match table{"nodes"=>"SELECT COUNT(*) FROM graph_nodes","edges"=>"SELECT COUNT(*) FROM graph_edges","stack"=>"SELECT COUNT(*) FROM graph_nodes INDEXED BY graph_discovery_stack WHERE (flags&8)=8",_=>panic!("closed diagnostic table")})
}
fn check_indexes(path: &std::path::Path) {
    let connection = external(path);
    let names:Vec<String>=connection.prepare("SELECT name FROM sqlite_schema WHERE type='index' AND tbl_name='graph_nodes' ORDER BY name").unwrap().query_map([],|row|row.get(0)).unwrap().map(Result::unwrap).collect();
    assert_eq!(names, vec!["graph_discovery_stack", "graph_unexpanded"]);
    for sql in ["EXPLAIN QUERY PLAN SELECT key FROM graph_nodes INDEXED BY graph_unexpanded WHERE (flags&2)=0 ORDER BY key LIMIT 1","EXPLAIN QUERY PLAN SELECT key FROM graph_nodes INDEXED BY graph_discovery_stack WHERE (flags&8)=8 AND discovery>=1 ORDER BY discovery DESC LIMIT 128","EXPLAIN QUERY PLAN SELECT key FROM graph_edges WHERE key>x'00' ORDER BY key LIMIT 128"] {let details:Vec<String>=connection.prepare(sql).unwrap().query_map([],|row|row.get(3)).unwrap().map(Result::unwrap).collect();assert!(details.iter().all(|detail|!detail.contains("TEMP B-TREE")),"{details:?}");assert!(details.iter().any(|detail|detail.contains("INDEX") || detail.contains("PRIMARY KEY")),"{details:?}");}
}
fn retire_roots(
    authority: &ScratchAuthority,
    session: &mut layerfs_storage::construction_state::ScratchSession,
    scopes: &GraphConstructionScopes,
    proof: &GraphProofSeal,
    initial: &layerfs_storage::construction_state::ScratchOwnerStatus,
) -> u64 {
    session.graph_retire(proof).unwrap();
    let connection = external(&initial.path);
    assert_eq!(scalar(&connection, "SELECT stage FROM graph_owner"), 7);
    assert_eq!(
        scalar(
            &connection,
            "SELECT remaining_nodes+remaining_edges FROM graph_owner"
        ),
        0
    );
    for sql in [
        "SELECT 1 FROM graph_nodes LIMIT 1",
        "SELECT 1 FROM graph_edges LIMIT 1",
        "SELECT 1 FROM graph_nodes INDEXED BY graph_unexpanded WHERE (flags&2)=0 LIMIT 1",
        "SELECT 1 FROM graph_nodes INDEXED BY graph_discovery_stack WHERE (flags&8)=8 LIMIT 1",
    ] {
        assert!(connection
            .query_row(sql, [], |_| Ok(()))
            .optional()
            .unwrap()
            .is_none());
    }
    let free = scalar(&connection, "PRAGMA freelist_count");
    drop(connection);
    let mut batch = Vec::with_capacity(128);
    let mut expected = roots_oracle::Transcript::new(roots_oracle::scope(
        initial.selector,
        initial.token,
        *session.selection().owner_binding().unwrap(),
        3,
    ));
    for serial in 1..=ROWS {
        batch.push(root(scopes.roots(), serial));
        expected.append(serial);
        if batch.len() == 128 {
            session.append(scopes.roots(), &batch).unwrap();
            batch.clear();
        }
    }
    assert!(batch.is_empty());
    assert_eq!(
        session.seal(scopes.roots()).unwrap().encode(),
        expected.seal()
    );
    let owner = status(authority, initial.token);
    assert_eq!(owner.file, initial.file);
    assert_eq!(
        owner.reserved_bytes,
        scopes.graph().capacity().scratch_bytes()
    );
    assert_eq!(owner.allocated_bytes, Some(owner.reserved_bytes));
    assert_eq!(authority.reserved_bytes().unwrap(), owner.reserved_bytes);
    assert!(
        scalar(&external(&owner.path), "PRAGMA page_count")
            <= scopes.graph().capacity().max_pages()
    );
    free
}

#[test]
fn maximum_default_node_population_reuses_widest_sites_then_maximum_roots() {
    with_source(ROWS as u32, true, |source, header| {
        let temp = TempDir::new("graph_maximum_nodes");
        let authority = ScratchAuthority::new(temp.path(), 1).unwrap();
        let (mut session, scopes) = begin(&authority, source, true, DEFAULT, ROWS, ROWS);
        let owner = status(&authority, session.selection().token());
        assert_header(temp.path(), &owner, scopes.graph());
        full_sites(&mut session, &scopes, header, ROWS as u32);
        let site_pages = scalar(&external(&owner.path), "PRAGMA page_count");
        let low = i64::MAX as u64 - ROWS + 1;
        seeds(&mut session, &scopes, low..=i64::MAX as u64);
        assert_eq!(count(&owner.path, "nodes"), ROWS);
        assert_eq!(count(&owner.path, "edges"), 0);
        check_indexes(&owner.path);
        let birth_pages = scalar(&external(&owner.path), "PRAGMA page_count");
        while let Some(parent) = session.graph_unexpanded(scopes.graph()).unwrap() {
            session.graph_expanded(scopes.graph(), &parent).unwrap();
        }
        let seal = session.graph_seal(scopes.graph()).unwrap();
        let mut work = ValidationGraphWork::default();
        solve_effective_graph(&mut session.adapter(), &seal, &mut work).unwrap();
        assert_eq!((work.solver_entered, work.popped_members), (ROWS, ROWS));
        let proof = session.graph_finish(&seal).unwrap();
        let full_pages = scalar(&external(&owner.path), "PRAGMA page_count");
        let free = retire_roots(&authority, &mut session, &scopes, &proof, &owner);
        let final_pages = scalar(&external(&owner.path), "PRAGMA page_count");
        assert!([site_pages, birth_pages, full_pages, final_pages]
            .into_iter()
            .all(|pages| pages <= 4096));
        eprintln!("LFCS4 native-format maximum node diagnostic: S={DEFAULT},R={ROWS},nodes={ROWS},edges=0,Node60={},widest_parent_sites_pages={site_pages},unexpanded_index_pages={birth_pages},full_rank_pages={full_pages},retired_free={free},roots={ROWS},final_pages={final_pages},same_file={:?},actual_Rust_Node={}B,Edge={}B; caller fixtures/oracles excluded; no whole-C1 work/global/RAM/protected-progress/speed claim",ROWS*60,owner.file,std::mem::size_of::<GraphNode>(),std::mem::size_of::<GraphEdge>());
        session.release().unwrap();
        assert_eq!(authority.reserved_bytes().unwrap(), 0);
    });
}

#[test]
fn maximum_default_deep_chain_fills_live_stack_and_widest_pointer_fields() {
    with_source(1, true, |source, _| {
        let temp = TempDir::new("graph_maximum_stack");
        let authority = ScratchAuthority::new(temp.path(), 1).unwrap();
        let (mut session, scopes) = begin(&authority, source, true, DEFAULT, ROWS, 1);
        empty_sites(&mut session, &scopes);
        let owner = status(&authority, session.selection().token());
        let n = 32768u64;
        let low = i64::MAX as u64 - n + 1;
        let high = i64::MAX as u64;
        let seal = graph(&mut session, &scopes, &[low], |serial| {
            if serial < high {
                vec![serial + 1]
            } else {
                vec![]
            }
        });
        assert_eq!((seal.nodes(), seal.edges()), (n, n - 1));
        session.graph_begin_scc(&seal).unwrap();
        let before = session
            .graph_node(&seal, GraphNodeKey::new(scopes.graph(), low).unwrap())
            .unwrap()
            .unwrap();
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
        for serial in low + 1..=high {
            let child = session
                .graph_node(&seal, GraphNodeKey::new(scopes.graph(), serial).unwrap())
                .unwrap()
                .unwrap();
            let edge = session
                .graph_edge_page(
                    &seal,
                    serial - 1,
                    None,
                    GraphPageLimit::new(1, 361).unwrap(),
                )
                .unwrap()
                .records()[0];
            let parent_after = active
                .advance(scopes.graph(), serial, active.lowlink())
                .unwrap();
            let child_after = child
                .enter(scopes.graph(), (serial - low + 1) as u32, serial - 1)
                .unwrap();
            session
                .graph_cas(
                    &seal,
                    &[GraphMutation::Descend {
                        parent_before: active,
                        parent_after,
                        child_before: child,
                        child_after,
                        edge,
                    }],
                )
                .unwrap();
            active = child_after;
        }
        assert_eq!(count(&owner.path, "stack"), n);
        check_indexes(&owner.path);
        let stack_pages = scalar(&external(&owner.path), "PRAGMA page_count");
        assert!(stack_pages <= 4096);
        assert_eq!(active.parent(), high - 1);
        assert_eq!(active.discovery(), n as u32);
        for serial in (low..=high).rev() {
            let before = session
                .graph_node(&seal, GraphNodeKey::new(scopes.graph(), serial).unwrap())
                .unwrap()
                .unwrap();
            let finished = before.finish(scopes.graph()).unwrap();
            session
                .graph_cas(
                    &seal,
                    &[GraphMutation::Finish {
                        before,
                        after: finished,
                    }],
                )
                .unwrap();
            let ack = session
                .graph_pop(&seal, &finished, GraphMutationLimit::default())
                .unwrap();
            assert_eq!(ack.disposition(), GraphPopDisposition::Complete);
            assert_eq!(ack.popped(), 1);
            if serial > low {
                let parent_before = session
                    .graph_node(
                        &seal,
                        GraphNodeKey::new(scopes.graph(), serial - 1).unwrap(),
                    )
                    .unwrap()
                    .unwrap();
                let child = ack.members()[0];
                let parent_after = parent_before.returned(scopes.graph(), child).unwrap();
                session
                    .graph_cas(
                        &seal,
                        &[GraphMutation::Return {
                            parent_before,
                            parent_after,
                            child: child.key(),
                        }],
                    )
                    .unwrap();
            } else {
                session
                    .graph_cas(
                        &seal,
                        &[GraphMutation::LeaveRoot {
                            finished_root: ack.members()[0],
                        }],
                    )
                    .unwrap();
            }
        }
        assert_eq!(count(&owner.path, "stack"), 0);
        let proof = session.graph_finish(&seal).unwrap();
        let free = retire_roots(&authority, &mut session, &scopes, &proof, &owner);
        eprintln!("LFCS4 native-format maximum depth diagnostic: S={DEFAULT},nodes={n},edges={},sum={},actual_stack={n},stack_pages={stack_pages},DFSparent/after actuali64MAX-adjacent,rank={n},retired_free={free},same_file={:?}; independent arithmetic/closed real native transitions; no C1 work/global/RAM/speed claim",n-1,2*n-1,owner.file);
        session.release().unwrap();
    });
}

#[test]
fn maximum_default_edge_heavy_dag_keeps_one_aggregate_record_class() {
    with_source(512, true, |source, _| {
        let temp = TempDir::new("graph_maximum_edges");
        let authority = ScratchAuthority::new(temp.path(), 1).unwrap();
        let (mut session, scopes) = begin(&authority, source, true, DEFAULT, ROWS, 512);
        empty_sites(&mut session, &scopes);
        let owner = status(&authority, session.selection().token());
        let low = i64::MAX as u64 - 511;
        seeds(&mut session, &scopes, low..=i64::MAX as u64);
        while let Some(mut parent) = session.graph_unexpanded(scopes.graph()).unwrap() {
            if parent.key().serial() < low + 256 {
                for chunk in (low + 256..low + 510).collect::<Vec<_>>().chunks(63) {
                    let children: Vec<_> = chunk
                        .iter()
                        .map(|serial| GraphNodeKey::new(scopes.graph(), *serial).unwrap())
                        .collect();
                    parent = session
                        .graph_append(scopes.graph(), &parent, &children)
                        .unwrap()
                        .parent()
                        .unwrap();
                }
            }
            session.graph_expanded(scopes.graph(), &parent).unwrap();
        }
        let seal = session.graph_seal(scopes.graph()).unwrap();
        assert_eq!((seal.nodes(), seal.edges()), (512, 65024));
        assert_eq!(seal.nodes() + seal.edges(), ROWS);
        check_indexes(&owner.path);
        let edge_pages = scalar(&external(&owner.path), "PRAGMA page_count");
        let mut work = ValidationGraphWork::default();
        solve_effective_graph(&mut session.adapter(), &seal, &mut work).unwrap();
        assert_eq!(work.edge_steps, 65024);
        let proof = session.graph_finish(&seal).unwrap();
        let free = retire_roots(&authority, &mut session, &scopes, &proof, &owner);
        assert!(edge_pages <= 4096);
        eprintln!("LFCS4 native-format maximum edge diagnostic: S={DEFAULT},N=512,E=65024,sum={ROWS},full_frame_bytes={},edge_pages={edge_pages},retired_free={free},same_file={:?}; no enlarged class or whole-C1 work/global/RAM/speed claim",512*60+65024*43,owner.file);
        session.release().unwrap();
    });
}

#[test]
fn configured48_native_format_maximum_sum_retire_and_reuse_is_not_whole_c1_work_admission() {
    with_source(ROWS as u32, true, |source, _| {
        let temp = TempDir::new("graph_configured48");
        let authority = ScratchAuthority::new(temp.path(), 1).unwrap();
        let (mut session, scopes) = begin(&authority, source, true, CONFIGURED, ROWS, ROWS);
        empty_sites(&mut session, &scopes);
        let owner = status(&authority, session.selection().token());
        seeds(&mut session, &scopes, 1..=ROWS);
        while let Some(mut parent) = session.graph_unexpanded(scopes.graph()).unwrap() {
            if parent.key().serial() == 1 {
                let mut batch = Vec::with_capacity(63);
                for serial in ROWS + 1..=ROWS * 2 {
                    batch.push(GraphNodeKey::new(scopes.graph(), serial).unwrap());
                    if batch.len() == 63 || serial == ROWS * 2 {
                        parent = session
                            .graph_append(scopes.graph(), &parent, &batch)
                            .unwrap()
                            .parent()
                            .unwrap();
                        batch.clear();
                    }
                }
            }
            session.graph_expanded(scopes.graph(), &parent).unwrap();
        }
        let seal = session.graph_seal(scopes.graph()).unwrap();
        assert_eq!((seal.nodes(), seal.edges()), (ROWS * 2, ROWS));
        assert_eq!(
            seal.nodes() + seal.edges(),
            scopes.graph().capacity().records()
        );
        assert_eq!(session.profile().unwrap().max_page_count, 12288);
        let pages = scalar(&external(&owner.path), "PRAGMA page_count");
        let mut work = ValidationGraphWork::default();
        solve_effective_graph(&mut session.adapter(), &seal, &mut work).unwrap();
        let proof = session.graph_finish(&seal).unwrap();
        let free = retire_roots(&authority, &mut session, &scopes, &proof, &owner);
        let final_pages = scalar(&external(&owner.path), "PRAGMA page_count");
        assert!(pages <= 12288 && final_pages <= 12288);
        eprintln!("LFCS4 configured native-format maximum diagnostic: S={CONFIGURED},R={},L={},N={},E={ROWS},sum={},pages={pages},retired_free={free},roots={ROWS},final_pages={final_pages},same_file={:?}; raw-base128Ki entry charge would exceed unchangedC1work65536, so this is strictly native-format/provider proof; configured whole-C1 case is separate; no global/RAM/speed claim",scopes.graph().capacity().records(),scopes.graph().capacity().encoded_bytes(),ROWS*2,ROWS*3,owner.file);
        session.release().unwrap();
        assert_eq!(authority.reserved_bytes().unwrap(), 0);
    });
}

#[test]
fn provably_new_maximum_capacity_refuses_before_query_reserve_attempt_under_exclusive() {
    with_source(ROWS as u32, true, |source, _| {
        let temp = TempDir::new("graph_maximum_refusal");
        let authority = ScratchAuthority::new(temp.path(), 1).unwrap();
        let (mut session, scopes) = begin(&authority, source, true, DEFAULT, 1, ROWS);
        empty_sites(&mut session, &scopes);
        seeds(&mut session, &scopes, 1..=ROWS);
        let parent = session.graph_unexpanded(scopes.graph()).unwrap().unwrap();
        let owner = status(&authority, session.selection().token());
        let connection = external(&owner.path);
        connection.execute_batch("BEGIN EXCLUSIVE").unwrap();
        let before_nodes = scalar(&connection, "SELECT nodes FROM graph_owner");
        let result = session.adapter().graph_append(
            scopes.graph(),
            &parent,
            &[GraphNodeKey::new(scopes.graph(), ROWS + 1).unwrap()],
        );
        assert!(matches!(
            result,
            Err(ContentError::ProviderFailure {
                what: "construction scratch state"
            })
        ));
        assert!(
            matches!(session.take_failure(),Some(StorageError::Content(ContentError::BoundedCapacityExceeded{what:"graph.records",limit:ROWS,actual}))if actual==ROWS+2)
        );
        assert_eq!(
            scalar(&connection, "SELECT nodes FROM graph_owner"),
            before_nodes
        );
        assert_eq!(
            scalar(&connection, "SELECT COUNT(*) FROM graph_nodes"),
            ROWS
        );
        assert_eq!(scalar(&connection, "SELECT COUNT(*) FROM graph_edges"), 0);
        assert_eq!(scalar(&connection, "SELECT stage FROM graph_owner"), 2);
        let failure = status(&authority, owner.token).failure.unwrap();
        assert!(!failure.contains("graph attempt"), "{failure}");
        connection.execute_batch("ROLLBACK").unwrap();
        drop(connection);
        assert_eq!(authority.reserved_bytes().unwrap(), DEFAULT);
        assert!(!session.is_quarantined());
        session.release().unwrap();
    });
}
