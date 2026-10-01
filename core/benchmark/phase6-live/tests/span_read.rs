//! Count-driven actual SQLite/file diagnostics, not timing or memory admission.
use phase6_live_probe::{engine::Engine, span_read};
use rusqlite::{params, StatementStatus};
fn fresh() -> (std::path::PathBuf, Engine) {
    let p = std::env::temp_dir().join(format!(
        "p6-span-{}",
        phase6_live_probe::minio::hex(&layerfs_sandbox::random::<16>().unwrap())
    ));
    std::fs::create_dir(&p).unwrap();
    let e = Engine::create(&p, 2).unwrap();
    (p, e)
}
fn late(count: i64) -> (u64, i32) {
    let (p, mut e) = fresh();
    let id = e.create_node(1, b"sparse", 1, 0o644).unwrap().id;
    for i in 0..count {
        e.write(id, i * 16, b"X").unwrap();
    }
    let at = (count - 1) * 16;
    let mut out = [0xFF; 4];
    assert_eq!(e.read(id, at, &mut out).unwrap(), 1);
    assert_eq!(out[0], b'X');
    assert_eq!(out[1], 0xFF);
    let work = e.span_work.get();
    assert_eq!(work.rows, 1);
    assert_eq!(work.predecessor_queries, 1);
    assert_eq!(work.range_queries, 1);
    let mut old=e.db.prepare("SELECT start,end,source,offset FROM extents WHERE ino=?1 AND start<?2 AND end>?3 ORDER BY start").unwrap();
    old.reset_status(StatementStatus::VmStep);
    let mut rows = old.query(params![id, at + 1, at]).unwrap();
    let mut found = 0;
    while rows.next().unwrap().is_some() {
        found += 1;
    }
    drop(rows);
    assert_eq!(found, 1);
    let old_steps = old.get_status(StatementStatus::VmStep);
    drop(old);
    let mut q =
        e.db.prepare(&format!("EXPLAIN QUERY PLAN {}", span_read::RANGE))
            .unwrap();
    let plan = q
        .query_map(params![id, at, at + 1], |r| r.get::<_, String>(3))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap()
        .join("\n");
    assert!(
        plan.contains("start>?") && plan.contains("start<?"),
        "{plan}"
    );
    assert!(!plan.contains("TEMP B-TREE"), "{plan}");
    drop(q);
    println!(
        "P6_SPAN_SQL_DIAGNOSTIC spans={count} old_vm={old_steps} new_vm={} rows={} plan={plan}",
        work.vm_steps, work.rows
    );
    let mut q=e.db.prepare("SELECT start,end,source,offset FROM extents WHERE ino=?1 AND start<?2 AND end>?2 ORDER BY start DESC LIMIT 1").unwrap();
    q.reset_status(StatementStatus::VmStep);
    let mut rows = q.query(params![id, at + 8]).unwrap();
    assert!(rows.next().unwrap().is_none());
    drop(rows);
    let old_boundary = q.get_status(StatementStatus::VmStep);
    drop(q);
    let before = e.mutation_work.get();
    e.write(id, at + 8, b"Y").unwrap();
    let after = e.mutation_work.get();
    let new_boundary = after.vm_steps - before.vm_steps;
    assert!(new_boundary < 128, "new mutation VM={new_boundary}");
    println!("P6_MUTATION_SQL_DIAGNOSTIC spans={count} old_boundary_vm={old_boundary} new_boundary_delete_vm={new_boundary}");
    drop(e);
    std::fs::remove_dir_all(p).unwrap();
    (work.vm_steps, old_steps)
}
#[test]
fn late_read_seeks_instead_of_scanning_4097_span_prefix() {
    let small = late(64);
    let large = late(4097);
    assert!(large.0 <= small.0 + 24, "{small:?} {large:?}");
    assert!(large.0 < 128);
    assert!(large.1 > small.1 * 32);
}
#[test]
fn literal_holes_partial_sources_eof_and_zero_length_reads() {
    let (p, mut e) = fresh();
    let id = e.create_node(1, b"data", 1, 0o644).unwrap().id;
    e.write(id, 2, b"abcdef").unwrap();
    e.write(id, 4, b"ZZ").unwrap();
    e.write(id, 20, b"TAIL").unwrap();
    let mut expected = [0; 24];
    expected[2..8].copy_from_slice(b"abZZef");
    expected[20..24].copy_from_slice(b"TAIL");
    for at in [0, 1, 2, 3, 5, 6, 8, 19, 20, 22, 24, 30] {
        let mut out = vec![0xCC; 7];
        let n = e.read(id, at, &mut out).unwrap();
        let want = if at >= 24 {
            &[][..]
        } else {
            &expected[at as usize..(at as usize + 7).min(24)]
        };
        assert_eq!(&out[..n], want);
        assert!(out[n..].iter().all(|b| *b == 0xCC));
    }
    let work = e.span_work.get();
    assert_eq!(e.read(id, 0, &mut []).unwrap(), 0);
    assert_eq!(e.span_work.get().vm_steps, work.vm_steps);
    drop(e);
    std::fs::remove_dir_all(p).unwrap();
}
