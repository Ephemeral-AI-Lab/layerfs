//! #506 E2/E4: the history heads of a Store file change exactly with its
//! history and are the same in a snapshot; reading them leaves no sidecar
//! behind on a Store nobody holds; a snapshot reports its copied bytes.
#![cfg(any(target_os = "macos", target_os = "linux"))]
use crate::support;

use layerfs_history::records::{
    CommitMeta, CommitOutcome, CommitRequest, CommitVia, ForkRequest, ForkSource,
    StackInitialization,
};
use layerfs_history::{BranchId, HistoryCatalog, HistoryCatalogConfig};
use layerfs_persistence::{
    heads_digest, read_heads, PersistenceConfig, SqliteAcquisitionSchema, SqliteHandles,
    SqlitePackLayout, SqlitePersistenceProfile,
};
use layerfs_storage::StoragePolicy;
use support::{branch_id, name, root, stack, Temp};

const BINDING: &[u8] = b"heads-binding";
const CURSOR: [u8; 32] = [41; 32];

fn config(path: std::path::PathBuf) -> PersistenceConfig {
    PersistenceConfig::sqlite(path)
        .with_sqlite_profile(SqlitePersistenceProfile::Normal)
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
            required_head: None,
        })
        .unwrap();
    assert!(
        matches!(outcome, CommitOutcome::Committed { .. }),
        "{outcome:?}"
    );
}

/// A soft removal writes a marker and leaves the base row in place; the backup
/// heads digest must follow the visible history, not the leftover rows. A
/// removed Branch leaves its project's line and a removed project leaves
/// nothing, so a pushed copy's `layerfs-heads` names the live heads only.
#[test]
fn a_retired_project_or_branch_leaves_the_backup_heads_digest() {
    let temp = Temp::new("heads-retired");
    let path = temp.join("store");
    let handles = SqliteHandles::create(
        config(path.clone()),
        StoragePolicy::frozen_default(),
        &HistoryCatalogConfig {
            binding_key: BINDING.to_vec(),
            cursor_key: CURSOR,
            incarnation: 3,
        },
    )
    .unwrap();
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
    commit(&handles.history, branch, 5);
    assert_eq!(read_heads(&path).unwrap().len(), 2);
    let live = heads_digest(&path).unwrap();

    handles.history.mark_branch_removed(branch, 1_000).unwrap();
    // The marker is not a delete: `branch_any` still renders the row.
    assert!(handles.history.branch_any(branch).unwrap().is_some());
    let after_branch = read_heads(&path).unwrap();
    assert_eq!(after_branch.len(), 1, "{after_branch:?}");
    assert!(
        after_branch[0].starts_with("stack shop "),
        "{after_branch:?}"
    );
    let branch_removed = heads_digest(&path).unwrap();
    assert_ne!(branch_removed, live, "a removed Branch changes the digest");

    handles.history.mark_stack_deleted(stack(1), 1_000).unwrap();
    assert_eq!(
        handles.history.stack_deleted_ms(stack(1)).unwrap(),
        Some(1_000)
    );
    assert!(
        read_heads(&path).unwrap().is_empty(),
        "a removed project leaves no heads line"
    );
    let project_removed = heads_digest(&path).unwrap();
    assert_ne!(
        project_removed, branch_removed,
        "a removed project changes the digest"
    );
    assert_ne!(project_removed, live);
}

#[test]
fn heads_follow_history_and_match_the_snapshot() {
    let temp = Temp::new("heads");
    let path = temp.join("store");
    let handles = SqliteHandles::create(
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

fn create(path: &std::path::Path) -> SqliteHandles {
    SqliteHandles::create(
        config(path.to_path_buf()),
        StoragePolicy::frozen_default(),
        &HistoryCatalogConfig {
            binding_key: BINDING.to_vec(),
            cursor_key: CURSOR,
            incarnation: 3,
        },
    )
    .unwrap()
}

/// Runs `work` on a detached thread, so a wait that never ends cannot hold
/// the test past its bounds; process exit ends it.
fn started<T: Send + 'static>(
    work: impl FnOnce() -> T + Send + 'static,
) -> std::sync::mpsc::Receiver<T> {
    let (done, finished) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = done.send(work());
    });
    finished
}

const STILL_WAITING: std::time::Duration = std::time::Duration::from_millis(300);
const BOUND: std::time::Duration = std::time::Duration::from_secs(10);

/// #776 C3/C4: `read_heads` opens the Store read-write, and its close may be
/// SQLite's last, which checkpoints and removes `-wal`/`-shm`. It holds the
/// writer lane from before the open to after the close, so it waits while
/// another holder (here a publication lane; a compaction's install in the
/// finding) has it.
#[test]
fn reading_heads_waits_for_the_writer_lane() {
    let temp = Temp::new("heads-lane");
    let path = temp.join("store");
    let handles = create(&path);
    let lane = handles.history.publish_lane().unwrap();
    let reading = path.clone();
    let finished = started(move || read_heads(&reading));
    assert!(
        finished.recv_timeout(STILL_WAITING).is_err(),
        "read_heads opened the Store while another holder had the lane"
    );
    drop(lane);
    let heads = finished
        .recv_timeout(BOUND)
        .expect("read_heads takes the lane once it is free")
        .unwrap();
    assert!(heads.is_empty(), "{heads:?}");
}

/// #776 C3/S6: a read-only open of a Store nobody holds (no `-shm`) creates
/// the sidecars and runs SQLite's WAL recovery, so it waits for the lane.
/// The lane is the Store directory's: here another Store in the same folder
/// holds it, which leaves this Store's sidecars absent.
#[test]
fn a_read_only_open_of_an_unheld_store_waits_for_the_lane() {
    let temp = Temp::new("heads-read-only-lane");
    let unheld = temp.join("a");
    drop(create(&unheld));
    let mut shm = unheld.as_os_str().to_os_string();
    shm.push("-shm");
    assert!(
        !std::path::Path::new(&shm).exists(),
        "the closed Store keeps no -shm"
    );
    let other = create(&temp.join("b"));
    let lane = other.history.publish_lane().unwrap();
    let opening = unheld.clone();
    let finished = started(move || {
        SqliteHandles::open_read_only(config(opening), BINDING, CURSOR).map(|_| ())
    });
    assert!(
        finished.recv_timeout(STILL_WAITING).is_err(),
        "the read-only open created the sidecars while another holder had the lane"
    );
    drop(lane);
    finished
        .recv_timeout(BOUND)
        .expect("the read-only open takes the lane once it is free")
        .unwrap();
}

/// #776: the doctor's structure probe (`sqlite_structure`, the fallback when a
/// diagnostic open fails) opens the Store raw. On a Store nobody holds its
/// first read creates the sidecars and runs SQLite's WAL recovery, so it
/// follows the open rule and waits for the lane, as a read-only open does.
#[test]
fn the_structure_probe_of_an_unheld_store_waits_for_the_lane() {
    use layerfs_persistence::diagnostic::{sqlite_structure, Options, Outcome};
    let temp = Temp::new("heads-structure-lane");
    let unheld = temp.join("a");
    drop(create(&unheld));
    let mut shm = unheld.as_os_str().to_os_string();
    shm.push("-shm");
    assert!(
        !std::path::Path::new(&shm).exists(),
        "the closed Store keeps no -shm"
    );
    let other = create(&temp.join("b"));
    let lane = other.history.publish_lane().unwrap();
    let probing = unheld.clone();
    let finished = started(move || {
        let mut failed = false;
        let result = sqlite_structure(&probing, Options::default(), &mut |o| {
            failed |= o.outcome == Outcome::Fail;
        });
        (result, failed)
    });
    assert!(
        finished.recv_timeout(STILL_WAITING).is_err(),
        "the structure probe created the sidecars while another holder had the lane"
    );
    drop(lane);
    let (result, failed) = finished
        .recv_timeout(BOUND)
        .expect("the structure probe takes the lane once it is free");
    result.unwrap();
    assert!(!failed, "a sound Store has no structural failure");
}
