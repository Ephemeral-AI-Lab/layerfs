//! #768 verification finding 7: a mounted Workspace keeps its bound node's
//! removal markers for its Commits only within `KEPT_REMOVAL_BYTES`, on
//! SQLite and on PostgreSQL.
//!
//! Each marker is charged its path bytes and its record. The project gets
//! filler markers on long paths that name nothing (a marker need not name
//! an existing path) and one marker on `/a`, which exists. Within the bound
//! the mount keeps the markers and the first Commit reads nothing on the
//! control reader (stage 1); one filler more and the mount keeps nothing, so
//! that Commit reads the markers there again, as before stage 1. Either way
//! the Commit's whiteout step removes `/a`: its constructor returns the
//! bound root itself, ignoring the overlay's whiteout, so `/a` can only be
//! absent if the step removed it. The next Commit, over the Workspace's own
//! Commit, hides nothing and reads nothing in both cases. Before the bound,
//! the whole marker list stayed in memory for the mount's lifetime.
#[path = "support/adversary.rs"]
#[allow(dead_code)]
mod adversary;

use adversary::{verdict, write, Service, TestStore, Verdict};
use layerfs_bridge::control::SdkValue;
use layerfs_content::filesystem::FilesystemRootId;
use layerfs_history::PathRemovalRecord;
use layerfs_store_service::removal::KEPT_REMOVAL_BYTES;

const SEED: &[(&str, &[u8])] = &[("/a", b"a\n"), ("/b", b"b\n")];
/// A filler marker's relative path length: 16 components, 4,000 bytes.
const LONG: usize = 4_000;

/// A distinct 4,000-byte path (without the leading `/`) for filler `n`.
fn long(n: usize) -> String {
    let mut parts = vec![format!("f{n:03}{}", "x".repeat(246))];
    parts.extend((0..15).map(|_| "y".repeat(249)));
    let path = parts.join("/");
    assert_eq!(path.len(), LONG);
    path
}

/// What `fillers` long markers and the `/a` marker charge.
fn charged(fillers: usize) -> usize {
    let record = std::mem::size_of::<PathRemovalRecord>();
    fillers * (LONG + record) + ("a".len() + record)
}

/// Statements the control reader (session 0) ran so far.
fn control(service: &Service) -> u64 {
    service
        .service
        .opened
        .session_diagnostics(Some(0))
        .unwrap()
        .unwrap_or_default()
        .statements
}

fn remove(service: &Service, path: &str) {
    service
        .call("project.remove_file", vec![("path", SdkValue::text(path))])
        .unwrap_or_else(|refusal| panic!("remove {path}: {refusal:?}"));
}

/// Writes the markers, mounts, Commits twice and returns the control
/// reader's statements of each Commit.
fn run(store: &TestStore, fillers: usize) -> [u64; 2] {
    let branch = store.fork(&format!("work{fillers}"));
    let service = store.open();
    for n in 0..fillers {
        remove(&service, &format!("/{}", long(n)));
    }
    remove(&service, "/a");
    let workspace = service.bind(branch);
    let before = control(&service);
    let first = workspace
        .commit(|_, _, snapshot| Ok(FilesystemRootId(snapshot.effective_root)))
        .unwrap_or_else(|failure| panic!("first Commit: {failure:?}"));
    let first = (
        control(&service) - before,
        matches!(verdict(&Ok(first)), Verdict::Committed(_)),
    );
    assert!(
        first.1,
        "{:?} {fillers}: first Commit published",
        store.kind
    );
    let tree = store.tree(branch);
    assert!(
        !tree.contains_key("/a"),
        "{:?} {fillers}: /a hidden",
        store.kind
    );
    assert!(
        tree.contains_key("/b"),
        "{:?} {fillers}: /b kept",
        store.kind
    );
    write(&workspace, "/x", b"x\n");
    let before = control(&service);
    let second = adversary::commit(&workspace);
    assert!(matches!(verdict(&second), Verdict::Committed(_)));
    let second = control(&service) - before;
    let tree = store.tree(branch);
    assert!(!tree.contains_key("/a") && tree.contains_key("/x"));
    service.unmount(workspace);
    service.stop();
    [first.0, second]
}

#[test]
fn markers_within_the_bound_are_kept_and_over_it_are_read_per_commit() {
    adversary::tmp();
    // 64 fillers fit, 65 do not: the bound sits between them.
    assert!(charged(64) <= KEPT_REMOVAL_BYTES);
    assert!(charged(65) > KEPT_REMOVAL_BYTES);
    for store in TestStore::both("removals-bound", SEED) {
        let kept = run(&store, 64);
        println!("KEPT_REMOVALS {:?} within: control {kept:?}", store.kind);
        assert_eq!(kept, [0, 0], "{:?}: kept markers read nothing", store.kind);
        let over = run(&store, 65);
        println!("KEPT_REMOVALS {:?} over: control {over:?}", store.kind);
        assert!(
            over[0] > 0,
            "{:?}: markers over the bound are read by the Commit",
            store.kind
        );
        assert_eq!(over[1], 0, "{:?}: over its own Commit", store.kind);
    }
}
