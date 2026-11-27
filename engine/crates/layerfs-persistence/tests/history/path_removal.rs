//! `project.remove_file`: one marker, its Store-high-water frontier, the
//! `UpToDate` rule, the `/`-boundary subtree, and the per-project scope.
use crate::support;

use layerfs_history::error::Missing;
use layerfs_history::path::{path_key, trace_key, NormalizedPath};
use layerfs_history::records::{
    CommitMeta, CommitOutcome, CommitRecord, CommitRequest, CommitVia, ForkRequest, ForkSource,
    PathRemovalKind, PathRemovalRequest, StackInitialization,
};
use layerfs_history::{BranchId, HistoryCatalog, HistoryError, LayerStackId, NodeId};
use support::{branch_id, name, root, stack, Temp};

const REMOVED_AT: i64 = 1_700_000_000_000;

fn project(history: &dyn HistoryCatalog, n: u8, text: &str) -> (LayerStackId, BranchId) {
    let record = history
        .initialize_layerstack(&StackInitialization {
            logical: 0,
            stack: stack(n),
            name: name(text),
            scope: root(200),
            profile: root(201),
            genesis_root: root(n),
        })
        .unwrap();
    let branch = branch_id(n);
    history
        .fork(&ForkRequest {
            stack: stack(n),
            branch,
            name: name("work"),
            source: ForkSource::Layer(record.head_layer),
        })
        .unwrap();
    (stack(n), branch)
}

fn commit(history: &dyn HistoryCatalog, branch: BranchId, tag: u8) -> CommitRecord {
    let record = history.branch(branch).unwrap().unwrap();
    let current = match record.head_commit {
        Some(head) => history.commit(head).unwrap().unwrap().root,
        None => history.layer(record.base_layer).unwrap().unwrap().root,
    };
    let outcome = history
        .commit_branch(&CommitRequest {
            branch,
            expected_head: record.head_commit,
            expected_base: record.base_layer,
            expected_root: current,
            candidate_root: root(tag),
            profile: root(201),
            scope: root(200),
            second_parent: None,
            meta: CommitMeta::new(CommitVia::Files),
            force: false,
            required_head: None,
        })
        .unwrap();
    let CommitOutcome::Committed { record, .. } = outcome else {
        panic!("{outcome:?}");
    };
    *record
}

fn remove(
    history: &dyn HistoryCatalog,
    stack: LayerStackId,
    path: &str,
    at: i64,
) -> layerfs_history::PathRemovalOutcome {
    history
        .remove_path(&PathRemovalRequest {
            stack,
            path: NormalizedPath::new(path).unwrap().as_bytes().to_vec(),
            removed_ms: at,
        })
        .unwrap()
}

#[test]
fn the_marker_key_is_the_store_salted_trace_hash() {
    let temp = Temp::new("path-removal-key");
    let history = support::create(&temp.join("store"));
    let (shop, _) = project(&history, 1, "shop");
    remove(&history, shop, "/a/b", REMOVED_AT);
    // The row's key is `trace_key(trace_salt, stack, path)`, not a
    // Store-independent `BLAKE3(path)` (§4.3, "traces record a hash of the
    // path").
    let connection = rusqlite::Connection::open(temp.join("store")).unwrap();
    let key: Vec<u8> = connection
        .query_row(
            "SELECT path_key FROM path_removed WHERE layer_stack_id=?1",
            rusqlite::params![shop.as_slice()],
            |row| row.get(0),
        )
        .unwrap();
    let salt: Vec<u8> = connection
        .query_row(
            "SELECT trace_salt FROM store_policy WHERE id=1",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let salt: [u8; 32] = salt.try_into().unwrap();
    assert_eq!(key, trace_key(&salt, shop, b"a/b").to_vec());
    assert_ne!(key, path_key(b"a/b").to_vec());
}

#[test]
fn marker_frontier_and_up_to_date_follow_the_store_position() {
    let temp = Temp::new("path-removal");
    let history = support::create(&temp.join("store"));
    let (shop, branch) = project(&history, 1, "shop");
    let genesis = history.layer_stack(shop).unwrap().unwrap().head_layer;
    assert_eq!(
        history.node_history_serial(NodeId::Layer(genesis)).unwrap(),
        Some(1)
    );
    let first = commit(&history, branch, 5);
    assert_eq!(
        history
            .node_history_serial(NodeId::Commit(first.id))
            .unwrap(),
        Some(2)
    );
    let _second = commit(&history, branch, 6);

    let removed = remove(&history, shop, "/a", REMOVED_AT);
    assert_eq!(removed.kind, PathRemovalKind::Removed);
    assert_eq!(removed.stack, shop);
    assert_eq!(removed.path, b"a");
    assert_eq!(
        removed.frontier_serial, 3,
        "frontier is next_history_serial - 1"
    );
    assert_eq!(removed.removed_ms, REMOVED_AT);

    // No project node is above the marker frontier: the repeat is UpToDate
    // and keeps the stored values.
    let repeat = remove(&history, shop, "/a", REMOVED_AT + 1);
    assert_eq!(repeat.kind, PathRemovalKind::UpToDate);
    assert_eq!(repeat.frontier_serial, 3);
    assert_eq!(repeat.removed_ms, REMOVED_AT);

    // A commit in another project raises the Store high-water but leaves
    // this path UpToDate: it is not this project's node.
    let (_, api) = project(&history, 2, "api");
    let _api_commit = commit(&history, api, 7);
    let still = remove(&history, shop, "/a", REMOVED_AT + 2);
    assert_eq!(still.kind, PathRemovalKind::UpToDate);
    assert_eq!(still.frontier_serial, 3);

    // A new commit in this project advances the path.
    let _third = commit(&history, branch, 8);
    let advanced = remove(&history, shop, "/a", REMOVED_AT + 3);
    assert_eq!(advanced.kind, PathRemovalKind::Removed);
    assert_eq!(advanced.frontier_serial, 6);

    // One marker per (stack, path key); `a` and `ab` are different paths.
    let ab = remove(&history, shop, "/ab", REMOVED_AT + 4);
    assert_eq!(ab.kind, PathRemovalKind::Removed);
    assert_eq!(ab.frontier_serial, 6);
    let markers = history.path_removals(shop).unwrap();
    let paths: Vec<_> = markers.iter().map(|marker| marker.path.clone()).collect();
    assert_eq!(paths, vec![b"a".to_vec(), b"ab".to_vec()]);
    assert_eq!(markers[0].frontier_serial, 6);
    assert_eq!(markers[1].frontier_serial, 6);
    // `ab` is not governed by the `a` marker.
    assert!(!layerfs_history::path::governs(b"a", b"ab"));

    // The mount's one read (#768): the node's serial with every marker of
    // its project, the same rows the two separate reads return; another
    // project's node keeps its own serial and this project's markers are
    // not its own; an absent node is none.
    assert_eq!(
        history
            .node_path_removals(shop, NodeId::Commit(first.id))
            .unwrap(),
        Some((2, markers))
    );
    assert_eq!(
        history
            .node_path_removals(api_stack(&history, api), NodeId::Layer(genesis))
            .unwrap(),
        Some((1, Vec::new()))
    );
    assert_eq!(
        history
            .node_path_removals(shop, NodeId::Commit(absent_commit()))
            .unwrap(),
        None
    );
}

/// The stack of Branch `branch`.
fn api_stack(history: &dyn HistoryCatalog, branch: BranchId) -> LayerStackId {
    history.branch(branch).unwrap().unwrap().stack
}

#[test]
fn a_raw_marker_frontier_also_yields_up_to_date() {
    let temp = Temp::new("path-removal-raw");
    let history = support::create(&temp.join("store"));
    let (shop, _branch) = project(&history, 1, "shop");
    history
        .mark_path_removed(shop, b"old", 42, REMOVED_AT)
        .unwrap();
    let stored = history.path_removals(shop).unwrap();
    assert_eq!(stored.len(), 1);
    assert_eq!(stored[0].frontier_serial, 42);
    let current = remove(&history, shop, "old", REMOVED_AT + 1);
    assert_eq!(current.kind, PathRemovalKind::UpToDate);
    assert_eq!(current.frontier_serial, 42);
    assert_eq!(current.removed_ms, REMOVED_AT);
}

#[test]
fn an_unknown_project_is_missing() {
    let temp = Temp::new("path-removal-missing");
    let history = support::create(&temp.join("store"));
    assert_eq!(
        history.remove_path(&PathRemovalRequest {
            stack: stack(9),
            path: b"a".to_vec(),
            removed_ms: REMOVED_AT,
        }),
        Err(HistoryError::Missing(Missing::LayerStack))
    );
}

/// A Commit id no Store holds.
fn absent_commit() -> layerfs_history::CommitId {
    let mut bytes = [0x5a; 33];
    bytes[0] = 0x12;
    layerfs_history::CommitId::from_bytes(bytes).expect("a Commit id")
}
