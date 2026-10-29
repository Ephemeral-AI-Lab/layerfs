//! #506 E2/E4: the history heads of a Store file change exactly with its
//! history and are the same in a snapshot; reading them leaves no sidecar
//! behind on a Store nobody holds; a snapshot reports its copied bytes.
#![cfg(any(target_os = "macos", target_os = "linux"))]
mod support;

use layerfs_history::records::{
    CommitMeta, CommitOutcome, CommitRequest, CommitVia, ForkRequest, ForkSource,
    StackInitialization,
};
use layerfs_history::{BranchId, HistoryCatalog, HistoryCatalogConfig};
use layerfs_persistence::{
    read_heads, Handles, PersistenceConfig, SqliteAcquisitionSchema, SqlitePackLayout,
    SqlitePersistenceProfile,
};
use layerfs_storage::StoragePolicy;
use support::{branch_id, name, root, stack, Temp};

const BINDING: &[u8] = b"heads-binding";
const CURSOR: [u8; 32] = [41; 32];

fn config(path: std::path::PathBuf) -> PersistenceConfig {
    PersistenceConfig::sqlite(path)
        .with_sqlite_profile(SqlitePersistenceProfile::Disposable)
        .with_sqlite_pack_layout(SqlitePackLayout::GroupRowsIndexed)
        .with_sqlite_acquisition(SqliteAcquisitionSchema::Tables)
}

fn commit(history: &dyn HistoryCatalog, branch: BranchId, tag: u8) {
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
        })
        .unwrap();
    assert!(
        matches!(outcome, CommitOutcome::Committed { .. }),
        "{outcome:?}"
    );
}

#[test]
fn heads_follow_history_and_match_the_snapshot() {
    let temp = Temp::new("heads");
    let path = temp.join("store");
    let handles = Handles::create(
        config(path.clone()),
        StoragePolicy::frozen_default(),
        &HistoryCatalogConfig {
            binding_key: BINDING.to_vec(),
            cursor_key: CURSOR,
            incarnation: 3,
        },
    )
    .unwrap();
    assert_eq!(read_heads(&path).unwrap(), Vec::<String>::new());
    let record = handles
        .history
        .initialize_layerstack(&StackInitialization {
            logical: 10,
            stack: stack(1),
            name: name("shop"),
            scope: root(200),
            profile: root(201),
            genesis_root: root(1),
        })
        .unwrap();
    let project = read_heads(&path).unwrap();
    assert_eq!(project.len(), 1);
    assert!(project[0].starts_with("stack shop 32"), "{project:?}");
    let branch = branch_id(1);
    handles
        .history
        .fork(&ForkRequest {
            stack: stack(1),
            branch,
            name: name("work"),
            source: ForkSource::Layer(record.head_layer),
        })
        .unwrap();
    let forked = read_heads(&path).unwrap();
    assert_eq!(forked.len(), 2);
    assert!(forked[0].starts_with("branch shop work 32") && forked[0].ends_with(" -"));
    commit(&handles.history, branch, 5);
    let committed = read_heads(&path).unwrap();
    assert_ne!(committed, forked, "a Commit moves the Branch head");
    assert!(committed[0].contains(" 12"), "{committed:?}");
    // reading again changes nothing
    assert_eq!(read_heads(&path).unwrap(), committed);

    // the snapshot reports its bytes and has the same heads
    let mut seen = Vec::new();
    let snapshot = handles
        .snapshot_observed(&temp.join("copy"), &mut |done, total| {
            seen.push((done, total))
        })
        .unwrap();
    let (done, total) = *seen.last().unwrap();
    assert_eq!((done, total), (snapshot.bytes, snapshot.bytes));
    assert!(seen.windows(2).all(|w| w[0].0 <= w[1].0));
    assert_eq!(read_heads(&temp.join("copy")).unwrap(), committed);
    // a Store nobody holds keeps no sidecar after a read (Linux's bundled
    // SQLite; macOS's system one may keep a persistent WAL)
    #[cfg(target_os = "linux")]
    for suffix in ["-wal", "-shm"] {
        assert!(!temp.join(&format!("copy{suffix}")).exists(), "{suffix}");
    }
    assert!(read_heads(&temp.join("absent")).is_err());
}
