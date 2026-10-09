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
    // Full-window overwrite of the never-fragmented file: its four rows of
    // 32 KiB are written where they lie. Ten fixed statements of the job,
    // one read of row shapes per row; only the inode and Workspace rows
    // change. Before rows of several cells this was (74, 98).
    let over_fresh = work(&db, || fresh.write(0, &window));
    assert_eq!((over_fresh.0, over_fresh.2), (14, 2));
    // The same over 32 masked cells: per 32 KiB one read of the shapes, one
    // range delete of eight cells and one insert, each row change with its
    // accounting trigger program. The cost is fixed by the window, not by
    // the 65,536 earlier writes. It was (74, 98) as well.
    let over_dense = work(&db, || dense.write(0, &window));
    assert_eq!(
        (over_dense.0, over_dense.2),
        (10 + 4 * (3 + 8 + 1), 2 + 3 * (32 + 4))
    );
    dense.check();
    // The fragments are gone: the window is rows of 32 KiB like the fresh
    // file's, and a second overwrite costs exactly what the fresh one does.
    let over_again = work(&db, || dense.write(0, &window));
    assert_eq!((over_again.0, over_again.2), (over_fresh.0, over_fresh.2));
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
    // The window starts and ends inside a cell of a row of 32 KiB: both
    // parts are written in place after one point read each, and the whole
    // cells between them span three rows instead of four.
    assert_eq!(
        (edge_fresh.0, edge_fresh.2),
        (over_fresh.0 + 2 - 1, 2),
        "two edge-cell point reads"
    );
    dense.check();
    println!(
        "S5_FRAGMENTATION one_byte_writes=65536 per_write_statements={} per_write_vm={} overwrite_128k statements={} vm={} rows_changed={} never_fragmented_vm={} refragmented_overwrite_equals_never_fragmented=true edge_overwrite statements={} read_window_fragmented statements={} vm={} read_window_unfragmented statements={} vm={} fullscan=0 sorts=0",
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
