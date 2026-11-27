//! #41: one budget sizes every buffer and cache of a mark and of the
//! compaction around it, and only sizes them. Under the least budget a small
//! Store crosses every bound a large one crosses under the product budget:
//! the bits of the reached objects and the block identities are paged
//! through a file a page at a time, every sorter writes runs, the retained
//! packs are read back from a list. What the mark keeps, what the compaction
//! reports and every row of the Store it leaves are the same as under a
//! budget far wider than any product run has.
#![cfg(any(target_os = "macos", target_os = "linux"))]
#[path = "../support/mod.rs"]
mod support;

use layerfs_content::ObjectId;
use layerfs_persistence::CompactPath;
use support::doctor::Fixture;
use support::marks::{leaf, publish_files, retain, same_under_both_budgets, small_file};

/// Payload objects one extent page of this Store names: few, so that the
/// pages are more than the keys a sealed list remembers under the least
/// budget, and its stride doubles.
const EXTENTS: usize = 20;

/// 96,000 payload objects in 24 packs. Three in four of the first sixteen
/// packs' objects are named by a retained extent page, so those packs are
/// partly dead and are written again; the last eight packs are wholly dead,
/// so the compaction rebuilds. With the pages the Store locates more
/// objects than one page of block identities (65,536) or one page of the
/// reached bits (16,384) covers.
#[test]
fn a_rebuild_finds_the_same_under_the_least_and_a_wide_budget() {
    const FILES: u64 = 96_000;
    const LIVE_PACKS: usize = 16;
    let f = Fixture::new();
    let files: Vec<_> = (0..FILES).map(small_file).collect();
    let packs = publish_files(&f, &files);
    assert_eq!(packs.len(), 24);
    let live: Vec<ObjectId> = files[..LIVE_PACKS * 4_000]
        .iter()
        .enumerate()
        .filter(|(index, _)| index % 4 != 3)
        .map(|(_, file)| file.id())
        .collect();
    let leaves: Vec<ObjectId> = live.chunks(EXTENTS).map(|group| leaf(&f, group)).collect();
    retain(&f.handles, &leaves);
    let kept = live.len() + leaves.len();
    let store = f.temp.join("store.sqlite");
    let Fixture { handles, temp, .. } = f;
    drop(handles);

    let least = same_under_both_budgets(&store, temp.path());
    assert_eq!(least.mark.objects().len(), kept);
    assert_eq!(least.mark.objects().partly_dead_count(), LIVE_PACKS as u64);
    assert_eq!(least.receipt.path, CompactPath::Rebuild);
    assert_eq!(least.after.objects().len(), kept);
    assert_eq!(least.after.objects().partly_dead_count(), 0);
    assert!(least.mark.scratch_bytes() > 0, "the sorters wrote runs");
    let [missed, read, written, asked] = least.mark.paging();
    assert!(
        written > 6 && read > 6 && missed >= read && asked > missed,
        "the bits and the block identities went through their file: {missed} {read} {written}"
    );
}
