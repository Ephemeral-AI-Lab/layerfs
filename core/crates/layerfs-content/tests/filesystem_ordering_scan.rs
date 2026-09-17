//! One reader per tier: a lookup neither allocates nor rebuilds a reader.
//!
//! The run store keeps each tier's buffered reader alive across lookups, so
//! after the tiers are built an arbitrary mix of continuing and restarting
//! requests performs no heap allocation at all. A counting global allocator
//! makes that property observable from outside the store.

use std::alloc::{GlobalAlloc, Layout, System};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicUsize, Ordering as AtomicOrdering};

mod support;

use layerfs_content::filesystem::references::record::Row;
use layerfs_content::filesystem::references::runs::RunStore;
use layerfs_content::object::inode_leaf::InodeKind;
use support::filesystem::{synthetic, value, RecordingBacking, TempDir};

/// Counts allocation calls; every other behaviour is the system allocator's.
struct Counting;

static ALLOCATIONS: AtomicUsize = AtomicUsize::new(0);

// SAFETY: the counters are the only addition; allocation itself is forwarded
// unchanged to the system allocator.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, AtomicOrdering::Relaxed);
        // SAFETY: forwarded unchanged.
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        // SAFETY: forwarded unchanged.
        unsafe { System.dealloc(pointer, layout) }
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, AtomicOrdering::Relaxed);
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
    for batch in 0..BATCHES {
        let mut pending = BTreeMap::new();
        for index in 0..ROWS {
            let serial = batch * ROWS + index + 1;
            pending.insert(serial, row(serial));
        }
        store.spill(&pending).expect("spill");
    }
    let total = BATCHES * ROWS;
    assert!(
        store.live_runs() >= 2,
        "the fixture must leave several tiers live"
    );

    // Warm-up: the first lookup into a live tier creates that tier's scan and
    // its retained buffer. This is the only allocation the lookup path may
    // perform, it is bounded by the number of live tiers, and it happens once
    // per tier rather than once per lookup.
    let live_tiers = store.live_runs();
    let warmup_before = ALLOCATIONS.load(AtomicOrdering::Relaxed);
    for serial in [1, total / 2, total] {
        let found = store.find(serial).expect("find");
        assert_eq!(found.map(|row| row.serial()), Some(serial));
    }
    let warmup_allocated = ALLOCATIONS.load(AtomicOrdering::Relaxed) - warmup_before;
    assert!(
        warmup_allocated <= live_tiers * 2 + 4,
        "creating {live_tiers} tier scans allocated {warmup_allocated} times"
    );

    // An ascending sweep over every serial: the shape the reducer's demands
    // take. Each tier's scan reads its rows exactly once, so the sweep costs
    // one pass over the spilled rows, not one pass per serial.
    let read_before = store.work().rows_read;
    let allocations_before = ALLOCATIONS.load(AtomicOrdering::Relaxed);
    for serial in 1..=total {
        let found = store.find(serial).expect("find");
        assert_eq!(found.map(|row| row.serial()), Some(serial));
    }
    let sweep_reads = store.work().rows_read - read_before;
    assert_eq!(sweep_reads, total, "the ascending sweep is one pass");

    // Restarting requests: a serial the cursor already passed restarts the
    // tier's scan from the front, and the restarting wave also allocates
    // nothing - the restart keeps the tier's buffer.
    for serial in (1..=ROWS).rev() {
        let found = store.find(serial).expect("find");
        assert_eq!(found.map(|row| row.serial()), Some(serial));
    }

    let allocated = ALLOCATIONS.load(AtomicOrdering::Relaxed) - allocations_before;
    assert_eq!(
        allocated, 0,
        "the lookup wave allocated {allocated} times after the tier readers existed"
    );
}

#[test]
fn a_restarted_scan_returns_the_same_rows_as_a_fresh_one() {
    let temp = TempDir::new("ordering-scan-restart");
    let mut backing = RecordingBacking::new(temp.path());
    let mut store = RunStore::new(Some(&mut backing), 4096, 8 * 1024 * 1024);

    const ROWS: u64 = 96;
    let mut pending = BTreeMap::new();
    for serial in 1..=ROWS {
        pending.insert(serial, row(serial));
    }
    store.spill(&pending).expect("spill");

    // Walk forward, then demand the same serials again from behind the cursor:
    // every answer must equal what a scan started from the front returns.
    for serial in 1..=ROWS {
        let found = store.find(serial).expect("find");
        assert_eq!(found.map(|row| row.serial()), Some(serial));
    }
    for serial in (1..=ROWS).rev() {
        let found = store.find(serial).expect("find");
        assert_eq!(
            found.map(|row| row.serial()),
            Some(serial),
            "a restarted scan lost serial {serial}"
        );
    }
    // A serial no tier holds is absence, not an error.
    assert!(store.find(ROWS + 1).expect("find").is_none());
}
