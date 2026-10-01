//! Real freshly acknowledged process/kernel barriers for distinct graph commits.

use super::fixture::*;
use super::{process, support::TempDir};
use layerfs_content::filesystem::state::*;
use layerfs_content::filesystem::validate::{solve_effective_graph, ValidationGraphWork};
use layerfs_content::ContentError;
use layerfs_storage::construction_state::{ScratchAuthority, ScratchDisposition};
use layerfs_storage::StorageError;
use std::path::Path;

#[test]
fn actual_shared_reader_unknown_seed_append_expand_seal_solver_last_pop_proof_and_retire() {
    for phase in [
        "seeds",
        "close_seeds",
        "root",
        "append",
        "expanded",
        "adjacency",
        "begin_scc",
        "mutation",
        "last_pop",
        "proof",
        "retire",
    ] {
        let temp = TempDir::new("graph_unknown_process");
        let output = process::writer(temp.path(), phase);
        assert!(
            output.status.success(),
            "phase={phase} stdout={} stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(String::from_utf8_lossy(&output.stdout)
            .contains(&format!("LFCS4 exact Unknown retained: {phase}")));
        eprintln!("{}", String::from_utf8_lossy(&output.stderr));
        let private: Vec<_> = std::fs::read_dir(temp.path())
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .collect();
        assert_eq!(private.len(), 1);
        assert_eq!(
            std::fs::read_dir(&private[0]).unwrap().count(),
            1,
            "Unknown private owner was not unlinked: {phase}"
        );
    }
}

#[test]
fn writer_child() {
    let Some(base) = std::env::var_os("LAYERFS_GRAPH_UNKNOWN_BASE") else {
        return;
    };
    let phase = std::env::var("LAYERFS_GRAPH_UNKNOWN_PHASE").unwrap();
    let update = phase != "root";
    with_source(2, update, |source, _| {
        let authority = ScratchAuthority::new(Path::new(&base), 1).unwrap();
        let (mut session, scopes) = begin(&authority, source, update, DEFAULT, 1, 2);
        empty_sites(&mut session, &scopes);
        let owner = status(&authority, session.selection().token());
        let key = GraphNodeKey::new(scopes.graph(), 1).unwrap();
        let child = GraphNodeKey::new(scopes.graph(), 2).unwrap();
        let mut parent = None;
        let mut adjacency = None;
        let mut mutation = None;
        let mut pop_root = None;
        let mut proof = None;
        if !matches!(phase.as_str(), "seeds" | "root") {
            seeds(&mut session, &scopes, [1]);
            if phase != "close_seeds" {
                parent = session.graph_unexpanded(scopes.graph()).unwrap();
            }
        }
        if phase == "last_pop" {
            let p = parent.unwrap();
            parent = Some(
                session
                    .graph_append(scopes.graph(), &p, &[key])
                    .unwrap()
                    .parent()
                    .unwrap(),
            );
        }
        if matches!(
            phase.as_str(),
            "adjacency" | "begin_scc" | "mutation" | "last_pop" | "proof" | "retire"
        ) {
            let p = parent.unwrap();
            session.graph_expanded(scopes.graph(), &p).unwrap();
            if phase != "adjacency" {
                adjacency = Some(session.graph_seal(scopes.graph()).unwrap());
            }
        }
        if matches!(phase.as_str(), "mutation" | "last_pop" | "proof" | "retire") {
            let seal = adjacency.as_ref().unwrap();
            if matches!(phase.as_str(), "proof" | "retire") {
                let mut work = ValidationGraphWork::default();
                solve_effective_graph(&mut session.adapter(), seal, &mut work).unwrap();
                if phase == "retire" {
                    proof = Some(session.graph_finish(seal).unwrap());
                }
            } else {
                session.graph_begin_scc(seal).unwrap();
                let before = session.graph_node(seal, key).unwrap().unwrap();
                let active = before.enter(scopes.graph(), 1, 0).unwrap();
                if phase == "mutation" {
                    mutation = Some(GraphMutation::EnterRoot {
                        before,
                        after: active,
                    });
                } else {
                    session
                        .graph_cas(
                            seal,
                            &[GraphMutation::EnterRoot {
                                before,
                                after: active,
                            }],
                        )
                        .unwrap();
                    let edge = session
                        .graph_edge_page(seal, 1, None, GraphPageLimit::default())
                        .unwrap()
                        .records()[0];
                    let advanced = active.advance(scopes.graph(), 1, 1).unwrap();
                    session
                        .graph_cas(
                            seal,
                            &[GraphMutation::Advance {
                                before: active,
                                after: advanced,
                                edge,
                            }],
                        )
                        .unwrap();
                    let finished = advanced.finish(scopes.graph()).unwrap();
                    session
                        .graph_cas(
                            seal,
                            &[GraphMutation::Finish {
                                before: advanced,
                                after: finished,
                            }],
                        )
                        .unwrap();
                    pop_root = Some(finished);
                }
            }
        }
        let connection = external(&owner.path);
        let prior_nodes = scalar(&connection, "SELECT nodes FROM graph_owner");
        let prior_stage = scalar(&connection, "SELECT stage FROM graph_owner") as u8;
        let prior_edges = scalar(&connection, "SELECT edges FROM graph_owner");
        drop(connection);
        let mut reader = process::HeldReader::new(&owner.path, prior_nodes, prior_stage);
        let outcome = match phase.as_str() {
            "seeds" => session
                .adapter()
                .graph_seed_batch(scopes.graph(), &[key, child])
                .map(|_| ()),
            "close_seeds" => session
                .adapter()
                .graph_unexpanded(scopes.graph())
                .map(|_| ()),
            "root" => session.adapter().graph_root(scopes.graph()).map(|_| ()),
            "append" => session
                .adapter()
                .graph_append(scopes.graph(), &parent.unwrap(), &[child])
                .map(|_| ()),
            "expanded" => session
                .adapter()
                .graph_expanded(scopes.graph(), &parent.unwrap())
                .map(|_| ()),
            "adjacency" => session.adapter().graph_seal(scopes.graph()).map(|_| ()),
            "begin_scc" => session
                .adapter()
                .graph_begin_scc(adjacency.as_ref().unwrap()),
            "mutation" => session
                .adapter()
                .graph_cas(adjacency.as_ref().unwrap(), &[mutation.unwrap()])
                .map(|_| ()),
            "last_pop" => session
                .adapter()
                .graph_pop(
                    adjacency.as_ref().unwrap(),
                    &pop_root.unwrap(),
                    GraphMutationLimit::default(),
                )
                .map(|_| ()),
            "proof" => session
                .adapter()
                .graph_finish(adjacency.as_ref().unwrap())
                .map(|_| ()),
            "retire" => session.adapter().graph_retire(proof.as_ref().unwrap()),
            _ => panic!("unregistered phase"),
        };
        assert!(
            matches!(
                outcome,
                Err(ContentError::ProviderFailure {
                    what: "construction scratch state"
                })
            ),
            "{phase}: {outcome:?}"
        );
        let original = session.take_failure().unwrap();
        let StorageError::UnknownOutcome { original } = original else {
            panic!("expected actual COMMIT Unknown: {original:?}")
        };
        let StorageError::Engine(rusqlite::Error::SqliteFailure(code, _)) = *original else {
            panic!("expected actual engine error: {original:?}")
        };
        assert_eq!(code.code, rusqlite::ErrorCode::DatabaseBusy);
        assert!(session.is_quarantined());
        let before = status(&authority, owner.token);
        let metadata = std::fs::metadata(&owner.path).unwrap();
        session.graph_abandon(scopes.graph()).unwrap();
        session.graph_abandon(scopes.graph()).unwrap();
        let after = status(&authority, owner.token);
        assert_eq!(after.disposition, ScratchDisposition::Unknown);
        assert_eq!(after.failure, before.failure);
        let failure = after.failure.as_ref().unwrap();
        let kind = match phase.as_str() {
            "seeds" => "Seeds",
            "close_seeds" => "CloseSeeds",
            "root" => "Root",
            "append" => "Append",
            "expanded" => "Expanded",
            "adjacency" => "AdjacencySeal",
            "begin_scc" => "BeginScc",
            "mutation" => "Mutation",
            "last_pop" => "Pop",
            "proof" => "Proof",
            "retire" => "Retire",
            _ => unreachable!(),
        };
        assert!(
            failure.contains(&format!("graph attempt {kind}:")),
            "{failure}"
        );
        assert!(
            failure.contains(&format!("scope={:?}", scopes.graph().as_bytes())),
            "{failure}"
        );
        assert!(
            failure.contains(&format!(
                "stage={:?}",
                GraphStage::from_code(prior_stage).unwrap()
            )),
            "{failure}"
        );
        assert!(
            failure.contains(&format!("nodes: {prior_nodes}, edges: {prior_edges}")),
            "{failure}"
        );
        match phase.as_str() {
            "seeds" => {
                assert!(failure.contains("nodes: 0, edges: 0, multiplicity: 0"));
                assert!(failure.contains("nodes: 2, edges: 0, multiplicity: 0"));
                assert!(failure.contains("proposed_nodes=[Some(GraphNode"));
            }
            "append" => {
                assert!(failure.contains("proposed_edges=[Some(GraphEdge"));
                assert!(failure.contains("nodes: 2, edges: 1, multiplicity: 1"));
            }
            "mutation" => {
                assert!(failure.contains("mutation_codes=[1]"));
                assert!(failure.contains("next: 1"));
                assert!(failure.contains("next: 2"));
            }
            "last_pop" => {
                assert!(failure.contains("reject_cycle=true"));
                assert!(failure.contains("stage=Solving->Rejected"));
                assert!(failure.contains(&format!("selected_children=[{:?}]", pop_root)));
            }
            "adjacency" => {
                assert!(failure.contains("proposed_adjacency=Some(["));
            }
            "proof" => {
                assert!(failure.contains("proposed_proof=Some(["));
            }
            "retire" => {
                assert!(failure.contains("pending remaining_nodes=(1, 0)"));
                assert!(
                    failure.contains(&format!("proposed_node=Some({key:?})"))
                        || failure.contains(&format!("->(Some({key:?}),None)"))
                );
            }
            _ => {}
        }
        assert!(session.take_failure().is_none());
        assert_eq!(after.file, owner.file);
        assert_eq!(after.reserved_bytes, DEFAULT);
        assert_eq!(after.allocated_bytes, Some(DEFAULT));
        assert!(!after.release_attempted);
        let after_metadata = std::fs::metadata(&owner.path).unwrap();
        assert_eq!(identity(&metadata), identity(&after_metadata));
        assert_eq!(metadata.len(), after_metadata.len());
        use std::os::unix::fs::MetadataExt;
        assert_eq!(metadata.blocks(), after_metadata.blocks());
        assert_eq!(authority.reserved_bytes().unwrap(), DEFAULT);
        let output = reader.release_and_reap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        eprintln!("{}", String::from_utf8_lossy(&output.stderr));
        assert!(session.capacity(scopes.roots()).is_err());
        assert!(session.graph_select(scopes.graph()).is_err());
        assert!(session.release().is_err());
        assert!(session.take_failure().is_none());
        drop(session);
        let retained = status(&authority, owner.token);
        assert!(retained.retained && retained.quarantined);
        assert_eq!(retained.failure, after.failure);
        assert_eq!(retained.allocated_bytes, Some(DEFAULT));
        assert_eq!(authority.reserved_bytes().unwrap(), DEFAULT);
        assert!(authority.release_retained(owner.token).is_err());
        assert!(owner.path.exists());
        println!("LFCS4 exact Unknown retained: {phase}");
    });
}

#[test]
fn reader_child() {
    use std::io::{Read, Write};
    use std::os::unix::net::UnixStream;
    use std::time::Duration;
    let Some(path) = std::env::var_os("LAYERFS_GRAPH_READ_DB") else {
        return;
    };
    let socket = std::env::var_os("LAYERFS_GRAPH_READ_SOCKET").unwrap();
    let expected_nodes: i64 = std::env::var("LAYERFS_GRAPH_READ_RECORDS")
        .unwrap()
        .parse()
        .unwrap();
    let expected_stage: i64 = std::env::var("LAYERFS_GRAPH_READ_STAGE")
        .unwrap()
        .parse()
        .unwrap();
    let mut control = UnixStream::connect(Path::new(&socket)).unwrap();
    control
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    control
        .set_write_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let reader = rusqlite::Connection::open_with_flags(
        Path::new(&path),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NOFOLLOW,
    )
    .unwrap();
    reader.busy_timeout(Duration::ZERO).unwrap();
    let source: String = reader
        .query_row("SELECT sqlite_source_id()", [], |row| row.get(0))
        .unwrap();
    eprintln!(
        "LFCS4 actual reader SQLite={} source={source}; READ_ONLY|NOFOLLOW,busy0, executable={}",
        rusqlite::version(),
        std::env::current_exe().unwrap().display()
    );
    reader.execute_batch("BEGIN").unwrap();
    let facts: (i64, i64) = reader
        .query_row("SELECT nodes,stage FROM graph_owner", [], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })
        .unwrap();
    assert_eq!(facts, (expected_nodes, expected_stage));
    control.write_all(&[1]).unwrap();
    let mut command = [0];
    control.read_exact(&mut command).unwrap();
    assert_eq!(command, [2]);
    reader.execute_batch("ROLLBACK").unwrap();
    drop(reader);
    control.write_all(&[3]).unwrap();
}
