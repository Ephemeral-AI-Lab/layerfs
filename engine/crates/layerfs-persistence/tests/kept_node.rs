//! #41 T7.1: the §6.1 history closure. A mark keeps every non-retired
//! LayerStack and every Branch with no own or stack marker, then follows every
//! history edge to its fixpoint and records the result in `kept_node`. A
//! retired Branch a retained node reaches stays; a deleted project is
//! excluded; a Store written before the table gains it at its first writable
//! open.
#![cfg(any(target_os = "macos", target_os = "linux"))]
mod support;

use layerfs_history::{
    AddLayerOutcome, AddLayerRequest, BranchId, CommitMeta, CommitOutcome, CommitRecord,
    CommitRequest, CommitVia, ForkRequest, ForkSource, HistoryCatalog, HistoryCatalogConfig,
    LayerId, LayerRecord, LayerStackId, NodeId, StackInitialization,
};
use layerfs_persistence::{KeptNodeId, PersistenceConfig, SqliteHandles, SqlitePersistenceProfile};
use layerfs_storage::StoragePolicy;
use support::{branch_id, name, root, stack, Temp, BINDING};

fn config(path: &std::path::Path) -> PersistenceConfig {
    PersistenceConfig::sqlite(path).with_sqlite_profile(SqlitePersistenceProfile::Disposable)
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

fn writable(
    path: &std::path::Path,
) -> Result<SqliteHandles, layerfs_storage::port::PersistenceError> {
    SqliteHandles::open_writable(config(path), BINDING, [71; 32])
}

fn definition(sql: &rusqlite::Connection) -> String {
    let mut statement = sql
        .prepare("SELECT type,name,sql FROM sqlite_schema WHERE substr(name,1,7)!='sqlite_' ORDER BY type,name")
        .unwrap();
    let rows = statement
        .query_map([], |r| Ok([r.get::<_, String>(0)?, r.get(1)?, r.get(2)?]))
        .unwrap();
    rows.map(|r| r.unwrap().map(|v| v + "\n").concat())
        .collect()
}

/// Rewrites a fresh Store into the format written before T7.1: no
/// `kept_node` table, recorded as that revision wrote it. Returns the fresh
/// source string it replaced.
fn strip_kept_node(path: &std::path::Path) -> String {
    let sql = rusqlite::Connection::open(path).unwrap();
    let fresh: String = sql
        .query_row("SELECT schema_source FROM history_meta", [], |r| r.get(0))
        .unwrap();
    let start = fresh
        .find("CREATE TABLE kept_node (")
        .expect("a fresh Store holds the table");
    let end = start + fresh[start..].find(";\n").expect("the table ends") + 2;
    let old = format!("{}{}", &fresh[..start], &fresh[end..]);
    assert_ne!(old, fresh);
    sql.execute_batch("DROP TABLE kept_node;").unwrap();
    sql.execute(
        "UPDATE history_meta SET schema_source=?1,schema_definition=?2",
        rusqlite::params![old, definition(&sql)],
    )
    .unwrap();
    fresh
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

fn commit(handles: &SqliteHandles, branch: BranchId, tag: u8) -> CommitRecord {
    let snapshot = handles.history.branch(branch).unwrap().unwrap();
    let expected_root = match snapshot.head_commit {
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
    match handles
        .history
        .commit_branch(&CommitRequest {
            branch,
            expected_head: snapshot.head_commit,
            expected_base: snapshot.base_layer,
            expected_root,
            candidate_root: root(tag),
            profile: root(201),
            scope: root(200),
            second_parent: None,
            meta: CommitMeta::new(CommitVia::Files),
            force: false,
        })
        .unwrap()
    {
        CommitOutcome::Committed { record, .. } => *record,
        other => panic!("commit: {other:?}"),
    }
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

fn genesis(handles: &SqliteHandles, stack: LayerStackId) -> LayerId {
    let mut layer = handles
        .history
        .layer_stack(stack)
        .unwrap()
        .unwrap()
        .head_layer;
    loop {
        let record = handles.history.layer(layer).unwrap().unwrap();
        match record.parent {
            Some(parent) => layer = parent,
            None => return layer,
        }
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

#[test]
fn a_marker_free_branch_is_a_root() {
    let temp = Temp::new("kept-root");
    let path = temp.join("store.sqlite");
    let handles = create(&path);
    let one = initialize(&handles, 1);
    let work = branch_id(1);
    fork(&handles, one.id, work, 1);

    let kept = handles.history.mark_history_closure().unwrap();
    assert!(kept.contains(KeptNodeId::Stack(one.id)));
    assert!(kept.contains(KeptNodeId::Branch(work)));
    assert!(kept.contains(KeptNodeId::Layer(genesis(&handles, one.id))));
    assert_eq!(kept.len(), 3, "a stack, its genesis Layer and one Branch");
    assert!(kept.commits().next().is_none());

    // The returned set is exactly what the table holds, serials included.
    let rows = kept_rows(&path);
    assert_eq!(rows.len(), kept.len());
    for node in kept.nodes() {
        assert_eq!(node.stack(), one.id);
        let found = rows
            .iter()
            .find(|row| row.1 == node.id().kind() && row.2 == node.id().as_slice())
            .unwrap_or_else(|| panic!("{:?} not in kept_node", node.id()));
        assert_eq!(found.3, node.serial());
    }
    let genesis_node = kept
        .node(KeptNodeId::Layer(genesis(&handles, one.id)))
        .unwrap();
    let serial = handles
        .history
        .node_history_serial(NodeId::Layer(genesis(&handles, one.id)))
        .unwrap()
        .unwrap();
    assert_eq!(genesis_node.serial(), serial as i64);
    assert_eq!(kept.node(KeptNodeId::Branch(work)).unwrap().serial(), 0);
}

#[test]
fn a_retired_branch_a_retained_commit_reaches_is_kept() {
    let temp = Temp::new("kept-retired");
    let path = temp.join("store.sqlite");
    let handles = create(&path);
    let one = initialize(&handles, 1);
    let work = branch_id(1);
    fork(&handles, one.id, work, 1);
    let record = commit(&handles, work, 9);
    let layer = publish(&handles, one.id, work);

    handles.history.mark_branch_removed(work, 1_000).unwrap();
    assert!(
        handles.history.branch(work).unwrap().is_none(),
        "the marker hides the Branch from callers"
    );

    let kept = handles.history.mark_history_closure().unwrap();
    // A retained Commit names its Branch, so the retired row and its edges stay.
    assert!(kept.contains(KeptNodeId::Commit(record.id)));
    assert!(kept.contains(KeptNodeId::Branch(work)));
    assert!(kept.contains(KeptNodeId::Layer(record.base_layer)));
    assert!(kept.contains(KeptNodeId::Layer(layer.id)));
    assert!(kept.contains(KeptNodeId::Stack(one.id)));
    assert_eq!(
        kept.node(KeptNodeId::Commit(record.id)).unwrap().serial(),
        handles
            .history
            .node_history_serial(NodeId::Commit(record.id))
            .unwrap()
            .unwrap() as i64
    );
}

#[test]
fn a_retired_branch_nothing_reaches_is_not_kept() {
    let temp = Temp::new("kept-retired-unreferenced");
    let path = temp.join("store.sqlite");
    let handles = create(&path);
    let one = initialize(&handles, 1);
    let work = branch_id(1);
    fork(&handles, one.id, work, 1);
    let kept = handles.history.mark_history_closure().unwrap();
    assert!(kept.contains(KeptNodeId::Branch(work)));

    handles.history.mark_branch_removed(work, 1_000).unwrap();
    let kept = handles.history.mark_history_closure().unwrap();
    assert!(
        !kept.contains(KeptNodeId::Branch(work)),
        "a retired Branch no retained node reaches is collected"
    );
    assert!(kept.contains(KeptNodeId::Stack(one.id)));
}

#[test]
fn a_deleted_project_is_excluded() {
    let temp = Temp::new("kept-deleted");
    let path = temp.join("store.sqlite");
    let handles = create(&path);
    let one = initialize(&handles, 1);
    let two = initialize(&handles, 2);
    let first = branch_id(1);
    let second = branch_id(2);
    fork(&handles, one.id, first, 1);
    let first_commit = commit(&handles, first, 9);
    let first_layer = publish(&handles, one.id, first);
    fork(&handles, two.id, second, 2);
    let second_commit = commit(&handles, second, 10);
    let second_layer = publish(&handles, two.id, second);

    handles.history.mark_stack_deleted(one.id, 2_000).unwrap();
    let kept = handles.history.mark_history_closure().unwrap();

    assert!(!kept.contains(KeptNodeId::Stack(one.id)));
    assert!(!kept.contains(KeptNodeId::Commit(first_commit.id)));
    assert!(!kept.contains(KeptNodeId::Layer(first_layer.id)));
    assert!(!kept.contains(KeptNodeId::Branch(first)));
    assert!(
        kept.stacks().all(|stack| stack == two.id),
        "no node of the deleted project is reached"
    );
    assert!(kept.contains(KeptNodeId::Stack(two.id)));
    assert!(kept.contains(KeptNodeId::Commit(second_commit.id)));
    assert!(kept.contains(KeptNodeId::Layer(second_layer.id)));
    assert!(kept.contains(KeptNodeId::Branch(second)));
    assert!(kept_rows(&path)
        .iter()
        .all(|row| row.0 == two.id.as_slice()));
}

#[test]
fn a_mark_is_idempotent() {
    let temp = Temp::new("kept-idempotent");
    let path = temp.join("store.sqlite");
    let handles = create(&path);
    let one = initialize(&handles, 1);
    let work = branch_id(1);
    fork(&handles, one.id, work, 1);
    commit(&handles, work, 9);
    publish(&handles, one.id, work);

    let first = handles.history.mark_history_closure().unwrap();
    let rows = kept_rows(&path);
    // A stale row from an interrupted earlier mark is cleared, never kept.
    {
        let sql = rusqlite::Connection::open(&path).unwrap();
        sql.execute(
            "INSERT INTO kept_node(layer_stack_id,kind,node_id,serial) VALUES(?1,'branch',?2,0)",
            rusqlite::params![one.id.as_slice(), branch_id(9).as_slice()],
        )
        .unwrap();
    }
    let second = handles.history.mark_history_closure().unwrap();
    assert_eq!(first, second, "the same history yields the same set");
    assert_eq!(rows, kept_rows(&path), "the same rows are written");
    assert!(!second.contains(KeptNodeId::Branch(branch_id(9))));
}

#[test]
fn a_store_before_kept_node_gains_the_table() {
    let temp = Temp::new("kept-migration");
    let path = temp.join("store.sqlite");
    drop(create(&path));
    let fresh = strip_kept_node(&path);

    // The marker and its edges survive the migration; the table starts empty.
    let handles = writable(&path).expect("a pre-T7.1 Store opens writable");
    let one = initialize(&handles, 1);
    let work = branch_id(1);
    fork(&handles, one.id, work, 1);
    drop(handles);
    drop(writable(&path).expect("an upgraded Store reopens"));

    let sql = rusqlite::Connection::open(&path).unwrap();
    let kept: i64 = sql
        .query_row("SELECT count(*) FROM kept_node", [], |r| r.get(0))
        .unwrap();
    assert_eq!(kept, 0, "the migration writes no mark");
    let (source, recorded): (String, String) = sql
        .query_row(
            "SELECT schema_source,schema_definition FROM history_meta",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(source, fresh, "the Store records the current schema");
    assert_eq!(
        recorded,
        definition(&sql),
        "the recorded definition is the live one"
    );
    drop(sql);

    let handles = writable(&path).expect("a migrated Store reopens");
    let kept = handles.history.mark_history_closure().unwrap();
    assert!(kept.contains(KeptNodeId::Branch(work)));
    assert!(kept.contains(KeptNodeId::Stack(one.id)));
}

#[test]
fn an_empty_store_marks_nothing() {
    let temp = Temp::new("kept-empty");
    let path = temp.join("store.sqlite");
    let handles = create(&path);
    let kept = handles.history.mark_history_closure().unwrap();
    assert!(kept.is_empty());
    assert!(kept_rows(&path).is_empty());
}

#[test]
fn retained_roots_read_the_live_layers_and_removal_follows_the_marked_closure() {
    let temp = Temp::new("kept-roots-removal");
    let path = temp.join("store.sqlite");
    let handles = create(&path);
    let one = initialize(&handles, 1);
    let two = initialize(&handles, 2);
    let first = branch_id(1);
    fork(&handles, one.id, first, 1);
    let kept_commit = commit(&handles, first, 9);
    let kept_layer = publish(&handles, one.id, first);
    let second = branch_id(2);
    fork(&handles, two.id, second, 2);
    let gone_commit = commit(&handles, second, 10);
    let gone_layer = publish(&handles, two.id, second);

    // Before any mark the roots are computed from the live Stacks: every
    // Layer of a live Stack, and none of a retired one.
    let roots = handles.history.retained_roots(one.id).unwrap();
    assert!(roots.contains(&NodeId::Layer(kept_layer.id)));
    assert!(roots.contains(&NodeId::Layer(one.head_layer)));
    assert!(!roots.contains(&NodeId::Layer(gone_layer.id)));
    assert_eq!(
        handles.history.get_retained_roots(two.id).unwrap(),
        handles.history.retained_roots(two.id).unwrap()
    );

    handles.history.mark_history_closure().unwrap();
    assert_eq!(
        handles.history.get_retained_roots(one.id).unwrap(),
        roots,
        "the mark records the same roots it computed"
    );
    assert_eq!(
        handles.history.remove_unreachable(one.id, true).unwrap(),
        0,
        "a live project has no unreachable row"
    );

    // A deleted project's rows are unreachable once a mark records the live
    // closure; a dry run counts them and the real removal deletes them.
    handles.history.mark_stack_deleted(two.id, 2_000).unwrap();
    handles.history.mark_history_closure().unwrap();
    let dry = handles.history.remove_unreachable(two.id, true).unwrap();
    assert!(dry > 0, "the deleted project has rows to collect");
    assert_eq!(
        handles.history.remove_unreachable(two.id, false).unwrap(),
        dry
    );
    let sql = rusqlite::Connection::open(&path).unwrap();
    assert_eq!(
        sql.query_row(
            "SELECT count(*) FROM layer_stack WHERE layer_stack_id=?1",
            rusqlite::params![two.id.as_slice()],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        0,
        "the deleted project's row is gone"
    );
    assert_eq!(
        sql.query_row(
            "SELECT count(*) FROM \"commit\" WHERE commit_id=?1",
            rusqlite::params![gone_commit.id.as_slice()],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        0,
        "its Commits are gone"
    );
    assert_eq!(
        sql.query_row(
            "SELECT count(*) FROM layer WHERE layer_id=?1",
            rusqlite::params![gone_layer.id.as_slice()],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        0,
        "its Layers are gone"
    );
    drop(sql);

    // The live project and its history are untouched.
    assert!(handles.history.layer_stack(one.id).unwrap().is_some());
    assert!(handles.history.commit(kept_commit.id).unwrap().is_some());
    assert!(handles.history.layer(kept_layer.id).unwrap().is_some());
    assert_eq!(handles.history.remove_unreachable(one.id, true).unwrap(), 0);
    drop(handles);
    drop(writable(&path).expect("the Store still opens"));
}

#[test]
fn a_fork_created_after_a_mark_is_not_collected() {
    let temp = Temp::new("kept-late-fork");
    let path = temp.join("store.sqlite");
    let handles = create(&path);
    let one = initialize(&handles, 1);
    let early = branch_id(1);
    fork(&handles, one.id, early, 1);
    handles.history.mark_history_closure().unwrap();

    // The mark is older than this Branch: the prune must not trust it.
    let late = branch_id(2);
    fork(&handles, one.id, late, 2);
    assert!(handles.history.branch(late).unwrap().is_some());

    assert_eq!(handles.history.remove_unreachable(one.id, true).unwrap(), 0);
    assert_eq!(
        handles.history.remove_unreachable(one.id, false).unwrap(),
        0
    );
    assert!(
        handles.history.branch(late).unwrap().is_some(),
        "a live fork is never unreachable"
    );
    assert!(handles.history.branch(early).unwrap().is_some());
}

#[test]
fn a_project_initialized_after_a_mark_is_not_collected() {
    let temp = Temp::new("kept-late-project");
    let path = temp.join("store.sqlite");
    let handles = create(&path);
    let one = initialize(&handles, 1);
    handles.history.mark_history_closure().unwrap();

    // Init commits a new live root after the mark.
    let two = initialize(&handles, 2);
    assert_eq!(handles.history.remove_unreachable(two.id, true).unwrap(), 0);
    assert_eq!(
        handles.history.remove_unreachable(two.id, false).unwrap(),
        0
    );
    assert!(
        handles.history.layer_stack(two.id).unwrap().is_some(),
        "a live project is never unreachable"
    );
    assert!(handles.history.layer(two.head_layer).unwrap().is_some());
    assert!(handles.history.layer_stack(one.id).unwrap().is_some());
}

#[test]
fn a_commit_created_after_a_mark_is_kept_not_deleted() {
    let temp = Temp::new("kept-late-commit");
    let path = temp.join("store.sqlite");
    let handles = create(&path);
    let one = initialize(&handles, 1);
    let work = branch_id(1);
    fork(&handles, one.id, work, 1);
    handles.history.mark_history_closure().unwrap();

    // The mark is older than this Commit.
    let late = commit(&handles, work, 9);
    assert_eq!(
        handles.history.remove_unreachable(one.id, false).unwrap(),
        0
    );
    assert!(
        handles.history.commit(late.id).unwrap().is_some(),
        "a live Commit is never deleted"
    );
}

#[test]
fn a_store_whose_only_project_is_deleted_prunes_it_whole() {
    let temp = Temp::new("kept-only-deleted");
    let path = temp.join("store.sqlite");
    let handles = create(&path);
    let one = initialize(&handles, 1);
    let work = branch_id(1);
    fork(&handles, one.id, work, 1);
    let gone_commit = commit(&handles, work, 9);
    let gone_layer = publish(&handles, one.id, work);

    handles.history.mark_stack_deleted(one.id, 2_000).unwrap();
    let kept = handles.history.mark_history_closure().unwrap();
    assert!(kept.is_empty(), "no live project means an empty closure");

    // An empty closure is a real mark, not the absence of one: the whole
    // deleted project is counted and removed.
    let dry = handles.history.remove_unreachable(one.id, true).unwrap();
    assert_eq!(dry, 5, "one stack, one branch, two Layers and one Commit");
    assert_eq!(
        handles.history.remove_unreachable(one.id, false).unwrap(),
        dry
    );
    assert!(handles.history.layer_stack(one.id).unwrap().is_none());
    assert!(handles.history.commit(gone_commit.id).unwrap().is_none());
    assert!(handles.history.layer(gone_layer.id).unwrap().is_none());
    assert!(handles.history.branch(work).unwrap().is_none());
    drop(handles);
    drop(writable(&path).expect("the pruned Store still opens"));
}

#[test]
fn a_prune_after_a_cleared_mark_prunes_a_deleted_project() {
    let temp = Temp::new("kept-cleared-mark");
    let path = temp.join("store.sqlite");
    let handles = create(&path);
    let one = initialize(&handles, 1);
    handles.history.mark_history_closure().unwrap();
    handles.history.mark_stack_deleted(one.id, 2_000).unwrap();

    // A compaction clears the transient table. The prune must mark afresh
    // rather than read that empty table as "no mark" and do nothing.
    handles.history.clear_kept_node().unwrap();
    let dry = handles.history.remove_unreachable(one.id, true).unwrap();
    assert!(
        dry > 0,
        "a cleared closure still counts the deleted project"
    );
    assert_eq!(
        handles.history.remove_unreachable(one.id, false).unwrap(),
        dry
    );
    assert!(handles.history.layer_stack(one.id).unwrap().is_none());
}
