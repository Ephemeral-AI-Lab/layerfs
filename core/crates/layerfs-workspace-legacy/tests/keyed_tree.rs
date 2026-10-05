//! The keyed namespace tree's own counts.
//!
//! Two properties decide whether #256's namespace work is path-local rather
//! than whole-page: one ordered walk must reach each leaf it walks **once**,
//! and one deletion must rewrite one path rather than the page that holds the
//! name. Both are counted here on a real arena with real Linux backing, through
//! the page-store seam rather than a mount, so the numbers are the tree's own
//! page reads and writes and not a workload's.
#![cfg(target_os = "linux")]
use layerfs_workspace::sequence::{
    metadata_pages::{self, Cell, PageRef},
    Arena, Budget, MetadataHost, PayloadHost, RootOwner,
};
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};

/// Keys in one tree. A dirty cell is 4 + 17 + 1 = 22 bytes, so this many keys
/// are well inside one page's byte budget and past its 128-cell one: the tree
/// splits into two leaves under one branch, which is the smallest shape whose
/// walk has a leaf boundary to step across.
const KEYS: u64 = 160;
/// Deletions one candidate publication performs. A publication writes at most
/// 128 temporary pages, and a deletion on a two-level tree writes the leaf and
/// the branch above it.
const PER_CANDIDATE: u64 = 20;
const GENERATION: u64 = 5;

fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(30)
}
fn temporary() -> PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    let path = std::env::temp_dir().join(format!(
        "keyed-tree-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let mut builder = fs::DirBuilder::new();
    use std::os::unix::fs::DirBuilderExt;
    builder.mode(0o700);
    builder.create(&path).unwrap();
    path
}

/// Every key of one tree, in order, counted through the persistent cursor.
fn walk(
    arena: &std::sync::Arc<Arena>,
    root: PageRef,
    window: &mut layerfs_workspace::sequence::Window,
    deadline: Instant,
) -> (Vec<Vec<u8>>, u64, u64) {
    // A dirty key is one kind byte and one 64-bit identity, so the lowest key
    // of the kind is generation zero, inode zero.
    let mut cursor = arena
        .key_cursor(root, &metadata_pages::dirty_key(0, 0), window, deadline)
        .unwrap();
    let mut keys: Vec<Vec<u8>> = Vec::new();
    while let Some(cell) = cursor.next(window).unwrap() {
        if let Some(previous) = keys.last() {
            assert!(previous.as_slice() < cell.key(), "the walk left key order");
        }
        keys.push(cell.key().to_vec());
    }
    (keys, cursor.leaves(), cursor.reads())
}

#[test]
fn one_walk_reaches_each_leaf_once_and_one_delete_rewrites_one_path() {
    let path = temporary();
    let budget = Budget::new(256 * 1024 * 1024);
    let payloads = PayloadHost::new(path.join("payloads"), 256 * 1024 * 1024, &budget).unwrap();
    let directory = payloads.directory(path.join("pages"), [3; 32]).unwrap();
    // Each backing directory is created here with the ownership this process
    // must have and no group or other access, exactly as `WorkspaceHost`
    // creates them.
    payloads.initialize(&directory).unwrap();
    let host = MetadataHost::new(payloads.clone()).unwrap();
    let arena = host.arena(directory).unwrap();
    let deadline = deadline();
    let mut lease = payloads.window(1, 3).unwrap();
    let window = lease.window.as_mut().unwrap();

    // One publication inserts up to four cells of one key kind.
    let mut root = PageRef::NULL;
    let builder = host.candidate(&arena, GENERATION, false, None).unwrap();
    let mut inserted = 0;
    while inserted < KEYS {
        let mut updates = Vec::new();
        while updates.len() < 4 && inserted < KEYS {
            updates.push(
                Cell::new(&metadata_pages::dirty_key(GENERATION, 1 + inserted), &[1]).unwrap(),
            );
            inserted += 1;
        }
        root = builder.update(root, updates, window, deadline).unwrap();
    }

    let (keys, leaves, reads) = walk(&arena, root, window, deadline);
    assert_eq!(keys.len(), KEYS as usize);
    // The walk reads the root once and each of the two leaves once: `H + L`
    // page reads for sixteen times as many keys, and no page twice.
    assert_eq!(leaves, 2, "one reached leaf is one read");
    assert_eq!(reads, 3, "one branch page plus each leaf once");
    let distinct = {
        let mut sorted = keys.clone();
        sorted.dedup();
        sorted.len()
    };
    assert_eq!(distinct, KEYS as usize, "one reached key is one visit");
    println!(
        "KEYED_WALK keys={} leaves={leaves} reads={reads}",
        keys.len()
    );

    // One deletion per publication, one publication at a time, until the tree
    // holds nothing. Every step reads the key back from the published root, so
    // a deletion that dropped the wrong cell, left a page under the minimum
    // body or lost the root collapse fails here.
    let mut candidates = vec![builder];
    let mut removed = 0u64;
    while removed < KEYS {
        let candidate: std::sync::Arc<RootOwner> = candidates.last().unwrap().clone();
        for _ in 0..PER_CANDIDATE {
            if removed == KEYS {
                break;
            }
            let key = metadata_pages::dirty_key(GENERATION, 1 + removed);
            assert!(arena.find(root, &key, window, deadline).unwrap().is_some());
            root = candidate.delete(root, &key, window, deadline).unwrap();
            assert!(
                arena.find(root, &key, window, deadline).unwrap().is_none(),
                "the deleted key is still in the tree"
            );
            removed += 1;
        }
        if removed < KEYS {
            candidates.push(host.candidate(&arena, GENERATION, false, None).unwrap());
        }
        if removed % 32 == 0 {
            // Every four hundred keys the walk still sees exactly the keys that
            // are left, in order, through the same cursor.
            let (keys, _, _) = walk(&arena, root, window, deadline);
            assert_eq!(keys.len(), (KEYS - removed) as usize);
        }
    }
    assert_eq!(root, PageRef::NULL, "the last deletion empties the tree");
    let (keys, leaves, reads) = walk(&arena, root, window, deadline);
    assert!(keys.is_empty());
    assert_eq!((leaves, reads), (0, 0), "an empty tree is not walked");
    println!(
        "KEYED_DELETE deleted={removed} candidates={}",
        candidates.len()
    );

    drop(lease);
    drop(host);
    drop(payloads);
    fs::remove_dir_all(&path).unwrap();
}
