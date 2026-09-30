//! One reader per tier: a lookup neither allocates nor rebuilds a reader.
//!
//! The run store keeps each tier's buffered reader alive across lookups, so
//! after each tier's scan is admitted, continuing and independent point
//! requests perform no heap allocation. Observation is confined to the current
//! test thread and forwards every allocation unchanged to System.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::collections::BTreeMap;

mod support;

use layerfs_content::filesystem::references::record::Row;
use layerfs_content::filesystem::references::runs::RunStore;
use layerfs_content::object::inode_leaf::InodeKind;
use support::filesystem::{synthetic, value, RecordingBacking, TempDir};

/// Counts allocation calls; every other behaviour is the system allocator's.
struct Counting;

thread_local! {
    static ALLOCATIONS: Cell<Option<usize>> = const { Cell::new(None) };
}

fn count_allocation() {
    let _ = ALLOCATIONS.try_with(|count| {
        if let Some(value) = count.get() {
            count.set(Some(value + 1));
        }
    });
}

fn begin_count() {
    ALLOCATIONS.with(|count| count.set(Some(0)));
}
fn end_count() -> usize {
    ALLOCATIONS.with(|count| count.replace(None).unwrap())
}

// SAFETY: the counters are the only addition; allocation itself is forwarded
// unchanged to the system allocator.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        count_allocation();
        // SAFETY: forwarded unchanged.
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        // SAFETY: forwarded unchanged.
        unsafe { System.dealloc(pointer, layout) }
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        count_allocation();
        // SAFETY: forwarded unchanged.
        unsafe { System.realloc(pointer, layout, new_size) }
    }
}

#[global_allocator]
static GLOBAL: Counting = Counting;

fn row(serial: u64) -> Row {
    Row::Effect {
        serial,
        value: Some(value(
            InodeKind::RegularFile,
            synthetic(&format!("scan/{serial}")),
            synthetic("scan/meta"),
        )),
        delta: 1,
    }
}

#[test]
fn lookups_allocate_nothing_after_the_tiers_are_built() {
    let temp = TempDir::new("ordering-scan");
    let mut backing = RecordingBacking::new(temp.path());
    let mut store = RunStore::new(Some(&mut backing), 4096, 8 * 1024 * 1024);

    // Twelve spills of 64 rows each: the tier carries merge older batches like
    // a binary counter, so several tiers are live when the lookups begin (a
    // power-of-two count would collapse to one).
    const BATCHES: u64 = 12;
    const ROWS: u64 = 64;
    let mut expected = BTreeMap::new();
    for batch in 0..BATCHES {
        let mut pending = BTreeMap::new();
        for index in 0..ROWS {
            let serial = batch * ROWS + index + 1;
            let input_row = row(serial);
            expected.insert(serial, input_row);
            pending.insert(serial, input_row);
        }
        store.spill(&pending).expect("spill");
    }
    let total = BATCHES * ROWS;
    assert!(
        store.live_runs() >= 2,
        "the fixture must leave several tiers live"
    );

    // Fresh ascending demands pay for each tier's first retained scan and visit
    // each run row once. No prior high-key warmup can provide this one-pass proof.
    let live_tiers = store.live_runs();
    let read_before = store.work().rows_read;
    begin_count();
    for serial in 1..=total {
        let found = store.find(serial).expect("find");
        assert_eq!(found, expected.get(&serial).copied());
    }
    let scan_allocated = end_count();
    assert!(
        scan_allocated <= live_tiers * 2 + 4,
        "creating {live_tiers} tier scans allocated {scan_allocated} times"
    );
    assert_eq!(
        store.work().rows_read - read_before,
        total,
        "fresh ascending one-pass decode"
    );

    // All keys are now behind a permanently advanced high-water. These are
    // independent points, with logarithmic work and no prefix restart/allocation.
    let read_before = store.work().rows_read;
    begin_count();
    for serial in 1..=total {
        let found = store.find(serial).expect("find");
        assert_eq!(found, expected.get(&serial).copied());
    }
    for serial in (1..=ROWS).rev() {
        let found = store.find(serial).expect("find");
        assert_eq!(found, expected.get(&serial).copied());
    }
    let allocated = end_count();
    assert_eq!(
        allocated, 0,
        "the lookup wave allocated {allocated} times after the tier readers existed"
    );
    let queries = total + ROWS;
    let point_bound = u64::from(u64::BITS - total.leading_zeros()) + 1;
    assert!(store.work().rows_read - read_before <= queries * point_bound);
    store.release().expect("checked cleanup");
}

#[test]
fn descending_points_match_original_rows_after_a_fresh_forward_pass() {
    let temp = TempDir::new("ordering-scan-restart");
    let mut backing = RecordingBacking::new(temp.path());
    let mut store = RunStore::new(Some(&mut backing), 4096, 8 * 1024 * 1024);

    const ROWS: u64 = 96;
    let mut pending = BTreeMap::new();
    for serial in 1..=ROWS {
        pending.insert(serial, row(serial));
    }
    store.spill(&pending).expect("spill");

    // Expected full rows were supplied before any candidate lookup.
    for serial in 1..=ROWS {
        let found = store.find(serial).expect("find");
        assert_eq!(found, pending.get(&serial).copied());
    }
    for serial in (1..=ROWS).rev() {
        let found = store.find(serial).expect("find");
        assert_eq!(
            found,
            pending.get(&serial).copied(),
            "an independent point lost serial {serial}"
        );
    }
    // A serial no tier holds is absence, not an error.
    assert!(store.find(ROWS + 1).expect("find").is_none());
    store.release().expect("checked cleanup");
}

#[test]
fn a_sparse_newer_tier_skips_its_gap_and_finds_older_rows() {
    let temp = TempDir::new("ordering-scan-gap");
    let mut backing = RecordingBacking::new(temp.path());
    let mut store = RunStore::new(Some(&mut backing), 4096, 8 * 1024 * 1024);
    for range in [129..=192, 193..=256] {
        let pending = range.map(|serial| (serial, row(serial))).collect();
        store.spill(&pending).expect("older spill");
    }
    let mut newer = (1..=128)
        .map(|serial| (serial, row(serial)))
        .collect::<BTreeMap<_, _>>();
    newer.insert(300, row(300));
    store.spill(&newer).expect("newer spill");
    assert_eq!(store.live_runs(), 2);

    let before = store.work().rows_read;
    for serial in 129..=256 {
        assert_eq!(
            store.find(serial).unwrap().map(|row| row.serial()),
            Some(serial)
        );
    }
    assert!(
        store.work().rows_read - before < 500,
        "sparse misses must not rescan the low prefix"
    );
    assert!(store.find(299).unwrap().is_none());
    assert_eq!(store.find(300).unwrap().map(|row| row.serial()), Some(300));
    assert_eq!(store.find(2).unwrap().map(|row| row.serial()), Some(2));
}
