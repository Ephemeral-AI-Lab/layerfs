//! Consolidation of the tiered ordering store: the aliasing proof.

use std::collections::BTreeMap;

use layerfs_content::filesystem::references::backing::OrderingBacking;
use layerfs_content::filesystem::references::record::Row;
use layerfs_content::filesystem::references::runs::RunStore;
use layerfs_content::object::inode_leaf::InodeKind;
use support::filesystem::{synthetic, value, RecordingBacking, TempDir};

mod support;

fn row(serial: u64) -> Row {
    Row::Effect {
        serial,
        value: Some(value(
            InodeKind::RegularFile,
            synthetic(&format!("alias/{serial}")),
            synthetic("alias/meta"),
        )),
        delta: 1,
    }
}

/// Consolidation writes only into runs it created.
///
/// `consolidate()` used to copy the newest input into a fresh handle "so a merge
/// never aliases its own input". That copy rewrote every row of the newest run -
/// a whole run's read and write - to guard against a merge writing where it read.
/// `merge_runs` appends only to a run it created, so the guard is a property to
/// **prove**, not to assume: this case seals every run that exists before the
/// consolidation, which makes any append into one an error, and then requires the
/// consolidation to succeed and the row stream to be unchanged.
///
/// The seal is proven live in the same case: a handle taken before the seal
/// refuses an append after it, so a green run is not a dead detector.
///
/// This case lives in its own test binary on purpose: the sibling
/// `filesystem_ordering_scan.rs` carries an allocation-budget probe over a
/// **global** allocator, and a second test allocating in parallel disturbs it.
/// Its bound is a pinned case's, so the case that must not share a process with
/// it moved instead.
#[test]
fn consolidation_never_writes_into_a_run_it_merges() {
    let temp = TempDir::new("ordering-alias");
    let mut backing = RecordingBacking::new(temp.path());
    // Both handles are taken before the store borrows the backing: the control
    // run, and the probe the test reads and seals through.
    let mut control = OrderingBacking::create_run(&mut backing).expect("control run");
    let probe = backing.alias_probe();
    let mut store = RunStore::new(Some(&mut backing), 4096, 8 * 1024 * 1024);

    // Twelve spills of 64 rows: the tier carries leave several runs live.
    const BATCHES: u64 = 12;
    const ROWS: u64 = 64;
    for batch in 0..BATCHES {
        let mut pending = BTreeMap::new();
        for index in 0..ROWS {
            let mut entry = row(batch * ROWS + index + 1);
            // Every fourth batch updates a serial an earlier batch wrote, so the
            // consolidated stream is a real newest-wins merge and not a concat.
            if index % 16 == 0 && batch > 0 {
                entry = row(batch * ROWS + index + 1);
            }
            pending.insert(entry.serial(), entry);
        }
        store.spill(&pending).expect("spill");
    }
    assert!(
        store.live_runs() >= 2,
        "the fixture leaves several runs live"
    );

    let mut before = Vec::new();
    store
        .visit_newest_first(|row| {
            before.push(row);
            Ok(true)
        })
        .expect("view before consolidation");
    // The control handle was created before the seal and must refuse an append
    // after it: the detector is live, so a success below is evidence rather than
    // a silently dead probe.
    probe.seal();
    assert!(
        control.append(&row(1).encode().expect("encode")).is_err(),
        "a sealed run refuses an append"
    );
    drop(control);

    store
        .consolidate()
        .expect("consolidation writes only into runs it creates");

    let mut after = Vec::new();
    store
        .visit_newest_first(|row| {
            after.push(row);
            Ok(true)
        })
        .expect("view after consolidation");
    assert_eq!(after, before, "consolidation preserves the row stream");
    assert_eq!(store.live_runs(), 1, "one run survives");
}
