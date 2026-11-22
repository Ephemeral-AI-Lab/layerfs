//! #41 T6.5: the writer-side `project_epoch` fence (spec §5). Every operation
//! that creates a Commit or Layer — `commit_branch`, `add_layer`,
//! `merge_branch`, and the Workspace/file Commit path that calls
//! `commit_branch` — reads the project epoch in its write transaction and
//! re-checks it before that transaction commits. A change means a removal
//! marker (which bumps the epoch in its own transaction) raced the write; a
//! missing row means the project was deleted and compacted away. Both are
//! `Conflict`, and the whole transaction rolls back.
mod support;

use layerfs_history::error::{HistoryError, Missing};
use layerfs_history::records::{
    AddLayerOutcome, AddLayerRequest, BranchMergeRequest, CommitMeta, CommitOutcome, CommitRecord,
    CommitRequest, CommitVia, ForkRequest, ForkSource, MergeCommit, MergeHead, MergeSide,
    StackInitialization,
};
use layerfs_history::{BranchId, HistoryCatalog, HistoryCatalogConfig, LayerStackId, NodeId};
use layerfs_persistence::{PersistenceConfig, SqliteHandles, SqlitePersistenceProfile};
use support::{branch_id, name, root, stack, Temp};

const BINDING: &[u8] = b"epoch-fence";
const CURSOR: [u8; 32] = [53; 32];

fn config(path: &std::path::Path) -> PersistenceConfig {
    PersistenceConfig::sqlite(path).with_sqlite_profile(SqlitePersistenceProfile::Normal)
}

fn catalog(temp: &Temp) -> layerfs_persistence::HistoryProvider {
    support::sqlite::create(
        &temp.join("store.sqlite"),
        &HistoryCatalogConfig {
            cursor_key: CURSOR,
            binding_key: BINDING.to_vec(),
            incarnation: 3,
        },
    )
    .expect("catalog")
}

fn writable(path: &std::path::Path) -> layerfs_persistence::HistoryProvider {
    SqliteHandles::open_writable(config(path), BINDING, CURSOR)
        .expect("writable catalog")
        .history
}

/// Project `proj` with one Branch `work`, at its genesis Layer.
fn project(history: &dyn HistoryCatalog) -> (LayerStackId, BranchId, layerfs_history::LayerId) {
    let record = history
        .initialize_layerstack(&StackInitialization {
            logical: 1,
            stack: stack(1),
            name: name("proj"),
            scope: root(200),
            profile: root(201),
            genesis_root: root(1),
        })
        .unwrap();
    let branch = branch_id(1);
    history
        .fork(&ForkRequest {
            stack: stack(1),
            branch,
            name: name("work"),
            source: ForkSource::Layer(record.head_layer),
        })
        .unwrap();
    (stack(1), branch, record.head_layer)
}

fn commit_request(history: &dyn HistoryCatalog, branch: BranchId, tag: u8) -> CommitRequest {
    let record = history.branch(branch).unwrap().unwrap();
    let expected_root = match record.head_commit {
        Some(head) => history.commit(head).unwrap().unwrap().root,
        None => history.layer(record.base_layer).unwrap().unwrap().root,
    };
    CommitRequest {
        branch,
        expected_head: record.head_commit,
        expected_base: record.base_layer,
        expected_root,
        candidate_root: root(tag),
        profile: root(201),
        scope: root(200),
        second_parent: None,
        meta: CommitMeta::new(CommitVia::Files),
        force: false,
    }
}

fn commit(history: &dyn HistoryCatalog, branch: BranchId, tag: u8) -> CommitRecord {
    match history
        .commit_branch(&commit_request(history, branch, tag))
        .unwrap()
    {
        CommitOutcome::Committed { record, .. } => *record,
        other => panic!("commit: {other:?}"),
    }
}

fn layer_request(
    history: &dyn HistoryCatalog,
    stack: LayerStackId,
    branch: BranchId,
) -> AddLayerRequest {
    let record = history.branch(branch).unwrap().unwrap();
    let head = history.layer_stack(stack).unwrap().unwrap();
    AddLayerRequest {
        stack,
        branch,
        commit: record.head_commit.expect("a committed Branch"),
        expected_stack_head: head.head_layer,
        expected_branch_base: record.base_layer,
    }
}

fn publish(
    history: &dyn HistoryCatalog,
    stack: LayerStackId,
    branch: BranchId,
) -> layerfs_history::LayerRecord {
    match history
        .add_layer(&layer_request(history, stack, branch))
        .unwrap()
    {
        AddLayerOutcome::Added { layer } => layer,
        other => panic!("publish: {other:?}"),
    }
}

/// A merge of `work`'s Commit with a second Branch still at the genesis Layer:
/// one new Commit, second parent the genesis node, root `tag`.
fn merge_request(
    history: &dyn HistoryCatalog,
    stack: LayerStackId,
    branch: BranchId,
    commit: layerfs_history::CommitId,
    genesis: layerfs_history::LayerId,
    tag: u8,
) -> BranchMergeRequest {
    let side = branch_id(2);
    history
        .fork(&ForkRequest {
            stack,
            branch: side,
            name: name("side"),
            source: ForkSource::Layer(genesis),
        })
        .unwrap();
    BranchMergeRequest {
        stack,
        branch: branch_id(9),
        name: name("merged"),
        sides: [
            (MergeSide::Branch(branch), NodeId::Commit(commit)),
            (MergeSide::Branch(side), NodeId::Layer(genesis)),
        ],
        head: MergeHead::Commit(Box::new(MergeCommit {
            base: genesis,
            parent: Some(commit),
            second_parent: NodeId::Layer(genesis),
            root: root(tag),
            meta: CommitMeta::new(CommitVia::Merge),
        })),
    }
}

fn epoch(path: &std::path::Path, stack: LayerStackId) -> i64 {
    let connection = rusqlite::Connection::open(path).unwrap();
    connection
        .query_row(
            "SELECT epoch FROM project_epoch WHERE layer_stack_id = ?1",
            [stack.as_slice()],
            |row| row.get(0),
        )
        .unwrap()
}

/// `(commit rows, layer rows)` in the Store file, to prove a refusal wrote
/// nothing.
fn counts(path: &std::path::Path) -> (i64, i64) {
    let connection = rusqlite::Connection::open(path).unwrap();
    let count = |table: &str| -> i64 {
        connection
            .query_row(&format!("SELECT count(*) FROM \"{table}\""), [], |row| {
                row.get(0)
            })
            .unwrap()
    };
    (count("commit"), count("layer"))
}

fn trigger(path: &std::path::Path, table: &str) {
    let connection = rusqlite::Connection::open(path).unwrap();
    connection
        .execute_batch(&format!(
            "CREATE TRIGGER t65_epoch_race BEFORE INSERT ON \"{table}\" BEGIN \
             UPDATE project_epoch SET epoch = epoch + 1 \
             WHERE layer_stack_id = NEW.layer_stack_id; END;"
        ))
        .unwrap();
}

fn drop_trigger(path: &std::path::Path) {
    let connection = rusqlite::Connection::open(path).unwrap();
    connection
        .execute_batch("DROP TRIGGER t65_epoch_race")
        .unwrap();
}

#[test]
fn a_normal_commit_publish_and_merge_leave_the_epoch_alone() {
    let temp = Temp::new("epoch-fence-normal");
    let path = temp.join("store.sqlite");
    let history = catalog(&temp);
    let (shop, work, genesis) = project(&history);

    let first = commit(&history, work, 5);
    assert_eq!(epoch(&path, shop), 0, "a Commit does not bump the epoch");

    let layer = publish(&history, shop, work);
    assert_eq!(layer.parent, Some(genesis));
    assert_eq!(epoch(&path, shop), 0, "a published Layer does not bump it");

    let merge = history
        .merge_branch(&merge_request(&history, shop, work, first.id, genesis, 60))
        .unwrap();
    assert_eq!(merge.branch.name.as_str(), "merged");
    assert_eq!(epoch(&path, shop), 0, "a merge does not bump it");
    assert_eq!((counts(&path).0, counts(&path).1), (2, 2));
}

#[test]
fn a_missing_epoch_row_refuses_every_writer_with_conflict() {
    let temp = Temp::new("epoch-fence-missing");
    let path = temp.join("store.sqlite");
    let history = catalog(&temp);
    let (shop, work, genesis) = project(&history);
    let first = commit(&history, work, 5);
    let side_request = merge_request(&history, shop, work, first.id, genesis, 61);
    let before = counts(&path);

    let connection = rusqlite::Connection::open(&path).unwrap();
    connection
        .execute(
            "DELETE FROM project_epoch WHERE layer_stack_id = ?1",
            [shop.as_slice()],
        )
        .unwrap();
    drop(connection);

    // No fence row is left after `project.delete` + compaction: every writer
    // refuses `Conflict`, never "unchanged".
    let refused = history
        .commit_branch(&commit_request(&history, work, 6))
        .unwrap_err();
    assert!(
        matches!(refused, HistoryError::Conflict("project epoch")),
        "{refused:?}"
    );
    let refused = history
        .add_layer(&layer_request(&history, shop, work))
        .unwrap_err();
    assert!(
        matches!(refused, HistoryError::Conflict("project epoch")),
        "{refused:?}"
    );
    let refused = history.merge_branch(&side_request).unwrap_err();
    assert!(
        matches!(refused, HistoryError::Conflict("project epoch")),
        "{refused:?}"
    );

    // No partial write: no Commit, no Layer, and the Branch head is unchanged.
    assert_eq!(counts(&path), before);
    let snapshot = history.branch_snapshot(work).unwrap().unwrap();
    assert_eq!(snapshot.branch.head_commit, Some(first.id));
    assert_eq!(
        history.layer_stack(shop).unwrap().unwrap().head_layer,
        genesis
    );
    assert!(history.branch(branch_id(9)).unwrap().is_none());
}

#[test]
fn a_removal_first_refuses_the_later_commit_and_writes_nothing() {
    let temp = Temp::new("epoch-fence-removal-first");
    let path = temp.join("store.sqlite");
    let history = catalog(&temp);
    let (shop, work, _genesis) = project(&history);
    let _first = commit(&history, work, 5);
    let before = counts(&path);

    // Capture the request while the Branch is still visible; the removal then
    // wins, and the filtered Branch resolution hides the retired Branch before
    // the fence opens, so the write is refused and leaves no Commit behind.
    let request = commit_request(&history, work, 6);
    history.mark_branch_removed(work, 1_000).unwrap();
    assert_eq!(epoch(&path, shop), 1, "the marker bumped the epoch once");

    let refused = history.commit_branch(&request).unwrap_err();
    assert!(
        matches!(
            refused,
            HistoryError::Missing(Missing::Branch) | HistoryError::Conflict("project epoch")
        ),
        "{refused:?}"
    );
    assert_eq!(counts(&path), before);
}

#[test]
fn a_concurrent_commit_and_removal_have_one_winner() {
    let temp = Temp::new("epoch-fence-race");
    let path = temp.join("store.sqlite");
    let history = catalog(&temp);
    let (shop, work, _genesis) = project(&history);
    drop(history);

    let left = writable(&path);
    let right = writable(&path);
    let request = commit_request(&left, work, 7);
    let (committed, removed) = std::thread::scope(|scope| {
        let committed = scope.spawn(move || left.commit_branch(&request));
        let removed = scope.spawn(move || right.mark_branch_removed(work, 2_000));
        (committed.join().unwrap(), removed.join().unwrap())
    });

    // The two writes serialize: a Commit first, then the removal retires the
    // Branch with it (or is `Busy` and retries); or the removal first, and the
    // Commit is refused, leaving no row behind.
    match (&committed, &removed) {
        (Ok(_), Ok(_)) => {
            assert_eq!(epoch(&path, shop), 1);
            assert_eq!(counts(&path).0, 1, "the Commit stays under the removal");
        }
        (Ok(_), Err(HistoryError::Busy)) => {
            assert_eq!(epoch(&path, shop), 0, "the removal has to retry");
            assert_eq!(counts(&path).0, 1);
        }
        (
            Err(
                HistoryError::Busy
                | HistoryError::Missing(Missing::Branch)
                | HistoryError::Conflict(_),
            ),
            Ok(_),
        ) => {
            assert_eq!(epoch(&path, shop), 1);
            assert_eq!(counts(&path).0, 0, "the refused Commit left nothing");
        }
        other => panic!("unexpected outcome: {other:?}"),
    }
}

#[test]
fn an_epoch_change_during_a_commit_is_detected_and_rolled_back() {
    let temp = Temp::new("epoch-fence-trigger-commit");
    let path = temp.join("store.sqlite");
    let history = catalog(&temp);
    let (shop, work, _genesis) = project(&history);

    // A trigger stands in for the removal marker that bumps the epoch after
    // the writer's start read: it fires inside the writer's own transaction,
    // before its re-check.
    trigger(&path, "commit");
    let refused = history
        .commit_branch(&commit_request(&history, work, 5))
        .unwrap_err();
    assert!(
        matches!(refused, HistoryError::Conflict("project epoch")),
        "{refused:?}"
    );
    assert_eq!(counts(&path).0, 0, "the Commit rolled back");
    assert_eq!(epoch(&path, shop), 0, "the fake bump rolled back too");
    assert!(history.branch(work).unwrap().unwrap().head_commit.is_none());

    // Without the interleaved change the same Commit lands.
    drop_trigger(&path);
    let record = commit(&history, work, 5);
    assert_eq!(record.stack, shop);
    assert_eq!(epoch(&path, shop), 0);
    assert_eq!(
        history.branch(work).unwrap().unwrap().head_commit,
        Some(record.id)
    );
}

#[test]
fn an_epoch_change_during_a_publish_is_detected_and_rolled_back() {
    let temp = Temp::new("epoch-fence-trigger-layer");
    let path = temp.join("store.sqlite");
    let history = catalog(&temp);
    let (shop, work, genesis) = project(&history);
    let _first = commit(&history, work, 5);
    let before = counts(&path);

    trigger(&path, "layer");
    let refused = history
        .add_layer(&layer_request(&history, shop, work))
        .unwrap_err();
    assert!(
        matches!(refused, HistoryError::Conflict("project epoch")),
        "{refused:?}"
    );
    assert_eq!(counts(&path), before, "the Layer rolled back");
    assert_eq!(epoch(&path, shop), 0);
    assert_eq!(
        history.layer_stack(shop).unwrap().unwrap().head_layer,
        genesis
    );

    drop_trigger(&path);
    let layer = publish(&history, shop, work);
    assert_eq!(layer.parent, Some(genesis));
    assert_eq!(counts(&path).1, before.1 + 1);
}

#[test]
fn an_epoch_change_during_a_merge_is_detected_and_rolled_back() {
    let temp = Temp::new("epoch-fence-trigger-merge");
    let path = temp.join("store.sqlite");
    let history = catalog(&temp);
    let (shop, work, genesis) = project(&history);
    let first = commit(&history, work, 5);
    let request = merge_request(&history, shop, work, first.id, genesis, 62);
    let before = counts(&path);

    trigger(&path, "commit");
    let refused = history.merge_branch(&request).unwrap_err();
    assert!(
        matches!(refused, HistoryError::Conflict("project epoch")),
        "{refused:?}"
    );
    assert_eq!(counts(&path), before, "the merge Commit rolled back");
    assert_eq!(epoch(&path, shop), 0);
    assert!(history.branch(branch_id(9)).unwrap().is_none());

    drop_trigger(&path);
    let merge = history.merge_branch(&request).unwrap();
    assert_eq!(merge.branch.name.as_str(), "merged");
    assert!(merge.branch.head_commit.is_some());
}
