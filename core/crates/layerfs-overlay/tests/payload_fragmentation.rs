//! The 65536-write adversary runs independently of the other payload proofs.
mod payload_support;
use layerfs_overlay::*;
use payload_support::*;

#[test]
fn dense_fragmentation_does_not_enlarge_a_later_request() {
    let temp = Temp::new();
    let db = temp.db();
    let route = db.open_workspace([3; 32], [4; 32]).unwrap();
    let source = db.acquire_base_source(route, 1).unwrap();
    let plans = db.explain_payload(source).unwrap();
    for plan in &plans {
        assert!(
            plan.contains("SEARCH") && !plan.contains("SCAN") && !plan.contains("TEMP"),
            "{plan}"
        );
    }
    println!("S5_PLANS {plans:?}");
    let window = pattern(WRITE_WINDOW, 5);
    // A never-fragmented file, then a file of 65,536 alternating one-byte
    // writes over an inherited 128 KiB range: every other byte is a base byte.
    let mut fresh = File::new(&db, source, 20, Vec::new());
    fresh.write(0, &window);
    let mut dense = File::new(&db, source, 21, pattern(WRITE_WINDOW, 9));
    let fragments = work(&db, || {
        for at in (0..WRITE_WINDOW as u64).step_by(2) {
            dense.write(at, &[0]);
        }
    });
    dense.check();
    // The fragmented window is one inherited span, not one demand per gap.
    let local = db
        .source_read(source, 21, 0, READ_WINDOW as u32)
        .unwrap()
        .unwrap();
    assert_eq!(local.span, Some((1, WRITE_WINDOW as u64)));
    assert_eq!(
        local.inherited.iter().map(|b| b.count_ones()).sum::<u32>(),
        WRITE_WINDOW as u32 / 2
    );
    let read_dense = work(&db, || dense.check());
    let read_fresh = work(&db, || fresh.check());
    // Full-window overwrite: identical statement/change work and no cell read.
    // Accounting visits each old mask once; its NULL branch adds a fixed VM
    // step per aggregate update, independent of earlier fragment count.
    let over_fresh = work(&db, || fresh.write(0, &window));
    let over_dense = work(&db, || dense.write(0, &window));
    assert_eq!((over_dense.0, over_dense.2), (over_fresh.0, over_fresh.2));
    assert!(
        over_dense.1 <= over_fresh.1,
        "fragmentation enlarged VM work"
    );
    dense.check();
    assert!(db
        .source_read(source, 21, 0, READ_WINDOW as u32)
        .unwrap()
        .unwrap()
        .span
        .is_none());
    // An unaligned overwrite reads exactly its two edge cells on either file.
    let part = &window[..100_000];
    let edge_fresh = work(&db, || fresh.write(1_000, part));
    let edge_dense = work(&db, || dense.write(1_000, part));
    assert_eq!(edge_dense, edge_fresh);
    // Each cell DML also runs its accounting trigger. 32 covered cells
    // become 25 touched cells, with two additional edge-cell point reads.
    assert_eq!(
        edge_fresh.0,
        over_fresh.0 - 32 * 2 + 25 * 2 + 2,
        "two edge-cell point reads"
    );
    dense.check();
    println!(
        "S5_FRAGMENTATION one_byte_writes=65536 per_write_statements={} per_write_vm={} overwrite_128k statements={} vm={} rows_changed={} never_fragmented_vm={} same_statement_and_change_work=true edge_overwrite statements={} read_window_fragmented statements={} vm={} read_window_unfragmented statements={} vm={} fullscan=0 sorts=0",
        fragments.0 / 65536,
        fragments.1 / 65536,
        over_dense.0,
        over_dense.1,
        over_dense.2,
        over_fresh.1,
        edge_dense.0,
        read_dense.0,
        read_dense.1,
        read_fresh.0,
        read_fresh.1
    );
    assert_eq!(
        read_dense.0, read_fresh.0,
        "same statements to read either window"
    );
}
