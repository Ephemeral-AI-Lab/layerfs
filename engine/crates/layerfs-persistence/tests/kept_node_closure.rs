//! #41: the §6.1 history closure in bounded pieces. A SQLite mark does not
//! compute the closure in one recursive statement, which SQLite answers from
//! temporary tables in process memory that grow with the history. It seeds
//! `kept_node` with the live LayerStacks, their Layers and the live Branches,
//! then follows the edges of at most 256 kept nodes a statement, each
//! statement stopping after 8,192 rows, and sorts the nodes it has still to
//! follow in bounded memory and a sort file. These histories are deeper and
//! wider than those bounds, and the rows are the ones the single recursive
//! statement writes.
#![cfg(any(target_os = "macos", target_os = "linux"))]
mod support;

use layerfs_history::{
    AddLayerOutcome, AddLayerRequest, BranchId, CommitMeta, CommitOutcome, CommitRecord,
    CommitRequest, CommitVia, ForkRequest, ForkSource, HistoryCatalog, HistoryCatalogConfig,
    LayerRecord, LayerStackId, StackInitialization,
};
use layerfs_persistence::{KeptNodeId, PersistenceConfig, SqliteHandles, SqlitePersistenceProfile};
use layerfs_storage::StoragePolicy;
use support::{branch_id, name, root, stack, Temp, BINDING};

fn config(path: &std::path::Path) -> PersistenceConfig {
    PersistenceConfig::sqlite(path).with_sqlite_profile(SqlitePersistenceProfile::Normal)
}

fn create(path: &std::path::Path) -> SqliteHandles {
    SqliteHandles::create(
        config(path),
        StoragePolicy::frozen_default(),
        &HistoryCatalogConfig {
            binding_key: BINDING.to_vec(),
            cursor_key: [71; 32],
            incarnation: 7,
        },
    )
    .expect("Store creation")
}

fn initialize(handles: &SqliteHandles, tag: u8) -> layerfs_history::LayerStackRecord {
    handles
        .history
        .initialize_layerstack(&StackInitialization {
            logical: 10,
            stack: stack(tag),
            name: name(&format!("project-{tag}")),
            scope: root(200),
            profile: root(201),
            genesis_root: root(1),
        })
        .unwrap()
}

fn fork(handles: &SqliteHandles, stack: LayerStackId, branch: BranchId, tag: u8) {
    let record = handles.history.layer_stack(stack).unwrap().unwrap();
    handles
        .history
        .fork(&ForkRequest {
            stack,
            branch,
            name: name(&format!("branch-{tag}")),
            source: ForkSource::Layer(record.head_layer),
        })
        .unwrap();
}

fn publish(handles: &SqliteHandles, stack: LayerStackId, branch: BranchId) -> LayerRecord {
    let branch_record = handles.history.branch(branch).unwrap().unwrap();
    let stack_record = handles.history.layer_stack(stack).unwrap().unwrap();
    match handles
        .history
        .add_layer(&AddLayerRequest {
            stack,
            branch,
            commit: branch_record.head_commit.expect("a committed Branch"),
            expected_stack_head: stack_record.head_layer,
            expected_branch_base: branch_record.base_layer,
        })
        .unwrap()
    {
        AddLayerOutcome::Added { layer } => layer,
        other => panic!("publish: {other:?}"),
    }
}

fn kept_rows(path: &std::path::Path) -> Vec<(Vec<u8>, String, Vec<u8>, i64)> {
    let sql = rusqlite::Connection::open(path).unwrap();
    let mut statement = sql
        .prepare("SELECT layer_stack_id,kind,node_id,serial FROM kept_node ORDER BY layer_stack_id,kind,node_id")
        .unwrap();
    statement
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
        .unwrap()
        .map(|r| r.unwrap())
        .collect()
}

/// `count` Commits on `branch`, each over the one before, with roots no
/// other Commit of the Store has.
fn commits(handles: &SqliteHandles, branch: BranchId, seed: u64, count: u64) -> Vec<CommitRecord> {
    let snapshot = handles.history.branch(branch).unwrap().unwrap();
    let mut head = snapshot.head_commit;
    let mut expected_root = match head {
        Some(head) => handles.history.commit(head).unwrap().unwrap().root,
        None => {
            handles
                .history
                .layer(snapshot.base_layer)
                .unwrap()
                .unwrap()
                .root
        }
    };
    let mut made = Vec::new();
    for index in 0..count {
        let candidate = layerfs_content::ObjectId::for_bytes(
            &[seed.to_le_bytes(), index.to_le_bytes()].concat(),
        );
        match handles
            .history
            .commit_branch(&CommitRequest {
                branch,
                expected_head: head,
                expected_base: snapshot.base_layer,
                expected_root,
                candidate_root: candidate,
                profile: root(201),
                scope: root(200),
                second_parent: None,
                meta: CommitMeta::new(CommitVia::Files),
                force: false,
            })
            .unwrap()
        {
            CommitOutcome::Committed { record, .. } => {
                head = Some(record.id);
                expected_root = record.root;
                made.push(*record);
            }
            other => panic!("commit: {other:?}"),
        }
    }
    made
}

/// The closure the one recursive statement computes, on a connection of its
/// own and rolled back: the text the batched closure must agree with.
fn one_statement_closure(path: &std::path::Path) -> Vec<(Vec<u8>, String, Vec<u8>, i64)> {
    let sql = rusqlite::Connection::open(path).unwrap();
    sql.execute_batch("BEGIN; DELETE FROM kept_node;").unwrap();
    sql.execute(include_str!("fixtures/kept_node_one_statement.sql"), [])
        .unwrap();
    let rows = {
        let mut statement = sql
            .prepare("SELECT layer_stack_id,kind,node_id,serial FROM kept_node ORDER BY layer_stack_id,kind,node_id")
            .unwrap();
        statement
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
            .unwrap()
            .map(|r| r.unwrap())
            .collect()
    };
    sql.execute_batch("ROLLBACK").unwrap();
    rows
}

/// A SQLite mark computes the closure a bounded piece at a time: a statement
/// follows the edges of at most 256 kept nodes and stops after 8,192 rows.
/// This history is deeper than one statement reaches (one Branch of 9,000
/// Commits), has Branches forked along it, a published Layer, a removed
/// Branch nothing retains, removed Branches that a kept Commit or Layer
/// still names and a deleted project with a fork and a removed Branch of its
/// own: the rows are the ones the single recursive statement writes, and no
/// row is of a removed Branch nothing retains or of the deleted project.
#[test]
fn a_closure_in_batches_keeps_what_one_statement_keeps() {
    let temp = Temp::new("kept-batches");
    let path = temp.join("store.sqlite");
    let handles = create(&path);
    let one = initialize(&handles, 1);
    let main = branch_id(1);
    fork(&handles, one.id, main, 1);
    let chain = commits(&handles, main, 1, 9_000);
    // Branches forked from Commits along the chain, each with its own tail.
    let mut tails = Vec::new();
    for index in 0..40u8 {
        let branch = branch_id(10 + index);
        handles
            .history
            .fork(&ForkRequest {
                stack: one.id,
                branch,
                name: name(&format!("side-{index}")),
                source: ForkSource::Commit {
                    branch: main,
                    commit: chain[usize::from(index) * 200].id,
                },
            })
            .unwrap();
        tails.push(commits(&handles, branch, 100 + u64::from(index), 30));
    }
    // A removed Branch nothing retains goes with its own Commits; the
    // Commit of the chain it forked from stays, kept by the chain.
    let unretained = branch_id(10);
    handles
        .history
        .mark_branch_removed(unretained, 900)
        .unwrap();
    // A removed Branch another Branch forked from stays as a unit: the fork
    // keeps the Commit it forked from, and that Commit names the Branch.
    let forked_from = branch_id(11);
    let child = branch_id(250);
    handles
        .history
        .fork(&ForkRequest {
            stack: one.id,
            branch: child,
            name: name("child-of-removed"),
            source: ForkSource::Commit {
                branch: forked_from,
                commit: tails[1][10].id,
            },
        })
        .unwrap();
    handles
        .history
        .mark_branch_removed(forked_from, 901)
        .unwrap();
    // A published Layer, then more Commits over it and a Branch that is
    // retired while a kept Commit still names it.
    publish(&handles, one.id, main);
    let retired = branch_id(200);
    fork(&handles, one.id, retired, 200);
    commits(&handles, retired, 900, 5);
    publish(&handles, one.id, retired);
    handles.history.mark_branch_removed(retired, 1_000).unwrap();
    // A deleted project keeps nothing, forks and removed Branches included.
    let two = initialize(&handles, 2);
    let gone = branch_id(201);
    fork(&handles, two.id, gone, 201);
    let gone_chain = commits(&handles, gone, 901, 50);
    let gone_fork = branch_id(202);
    handles
        .history
        .fork(&ForkRequest {
            stack: two.id,
            branch: gone_fork,
            name: name("gone-fork"),
            source: ForkSource::Commit {
                branch: gone,
                commit: gone_chain[20].id,
            },
        })
        .unwrap();
    let gone_tail = commits(&handles, gone_fork, 902, 5);
    handles
        .history
        .mark_branch_removed(gone_fork, 1_500)
        .unwrap();
    handles.history.mark_stack_deleted(two.id, 2_000).unwrap();

    let kept = handles.history.mark_history_closure().unwrap();
    let rows = kept_rows(&path);
    assert_eq!(kept.len(), rows.len());
    assert_eq!(kept.nodes().len(), rows.len(), "a listing mark lists them");
    // Every tail but the one of the removed Branch nothing retains.
    assert!(kept.commits().count() >= 9_000 + 39 * 30);
    assert!(kept.contains(KeptNodeId::Branch(retired)));
    // The removed Branch nothing retains: its row and its own Commits are
    // gone, the Commit it forked from is not.
    assert!(!kept.contains(KeptNodeId::Branch(unretained)));
    assert!(tails[0]
        .iter()
        .all(|commit| !kept.contains(KeptNodeId::Commit(commit.id))));
    assert!(kept.contains(KeptNodeId::Commit(chain[0].id)));
    // The removed Branch a fork still reaches: its row, its head chain and
    // the fork.
    assert!(kept.contains(KeptNodeId::Branch(forked_from)));
    assert!(kept.contains(KeptNodeId::Branch(child)));
    assert!(tails[1]
        .iter()
        .all(|commit| kept.contains(KeptNodeId::Commit(commit.id))));
    // The deleted project: no node of it, whatever the node's kind.
    assert!(!kept.contains(KeptNodeId::Stack(two.id)));
    for branch in [gone, gone_fork] {
        assert!(!kept.contains(KeptNodeId::Branch(branch)));
    }
    assert!(gone_chain
        .iter()
        .chain(&gone_tail)
        .all(|commit| !kept.contains(KeptNodeId::Commit(commit.id))));
    assert!(
        rows.iter()
            .all(|row| row.0.as_slice() != two.id.to_bytes().as_slice()),
        "no kept row carries the deleted project's id"
    );
    drop(handles);
    let reference = one_statement_closure(&path);
    assert_eq!(rows.len(), reference.len());
    assert!(rows == reference, "the same rows as the one statement");
}

/// The nodes a closure has still to follow are sorted in bounded memory and
/// written to a sort file beyond it. A Store with more live Branches than
/// that memory holds starts with all of them to follow: every one is kept,
/// with the Commits behind them, and no sort file keeps a name.
#[test]
fn a_closure_wider_than_its_memory_keeps_every_root() {
    const BRANCHES: usize = 24_000;
    let temp = Temp::new("kept-wide");
    let path = temp.join("store.sqlite");
    let handles = create(&path);
    let one = initialize(&handles, 1);
    let main = branch_id(1);
    fork(&handles, one.id, main, 1);
    let chain = commits(&handles, main, 1, 9_000);
    for index in 0..BRANCHES {
        let mut authority = [0u8; 16];
        authority[..8].copy_from_slice(&(index as u64 + 1_000).to_le_bytes());
        handles
            .history
            .fork(&ForkRequest {
                stack: one.id,
                branch: BranchId::from_authority(authority),
                name: name(&format!("wide-{index}")),
                source: ForkSource::Commit {
                    branch: main,
                    commit: chain[index % chain.len()].id,
                },
            })
            .unwrap();
    }
    let kept = handles.history.mark_history_closure().unwrap();
    assert_eq!(kept.branches().count(), BRANCHES + 1);
    assert_eq!(kept.commits().count(), chain.len());
    let rows = kept_rows(&path);
    assert_eq!(kept.len(), rows.len());
    let left: Vec<_> = std::fs::read_dir(temp.path())
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|name| name.starts_with(".layerfs-mark-"))
        .collect();
    assert!(left.is_empty(), "no sort file keeps a name: {left:?}");
    drop(handles);
    assert!(rows == one_statement_closure(&path));
}
