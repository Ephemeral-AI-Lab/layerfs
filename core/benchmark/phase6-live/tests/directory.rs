//! Actual SQLite keysets, exact cookies and owned page bounds.
use phase6_live_probe::{
    directory::{self, PAGE_BYTES, PAGE_ROWS},
    engine::Engine,
};
fn fresh() -> (std::path::PathBuf, Engine) {
    let p = std::env::temp_dir().join(format!(
        "p6-dir-{}",
        phase6_live_probe::minio::hex(&layerfs_sandbox::random::<16>().unwrap())
    ));
    std::fs::create_dir(&p).unwrap();
    let e = Engine::create(&p, 2).unwrap();
    (p, e)
}
#[test]
fn multiple_pages_exact_inventory_seek_deleted_cookie_and_indexed_plan() {
    let (p, mut e) = fresh();
    let mut expected = std::collections::BTreeMap::new();
    for i in 0..400 {
        let name = format!("f{:03}", 399 - i);
        let id = e.create_node(1, name.as_bytes(), 1, 0o644).unwrap().id;
        expected.insert(name.into_bytes(), id);
    }
    let h = e.open(1, libc::O_RDONLY).unwrap();
    let mut after = 0;
    let mut actual = std::collections::BTreeMap::new();
    let mut saved = 0;
    loop {
        let page = e.directory_page(h, 1, after).unwrap();
        if page.is_empty() {
            break;
        }
        assert!(page.len() <= PAGE_ROWS);
        for row in page {
            assert!(row.cookie > after);
            after = row.cookie;
            if saved == 0 {
                saved = after;
            }
            assert!(actual.insert(row.name, row.inode).is_none());
        }
    }
    assert_eq!(actual, expected);
    assert!(e.directory_work.get().peak_rows <= PAGE_ROWS);
    assert!(e.directory_work.get().peak_payload_bytes <= PAGE_BYTES);
    assert!(
        e.directory_work.get().peak_owned_bytes
            <= PAGE_BYTES + PAGE_ROWS * std::mem::size_of::<directory::Entry>()
    );
    let repeated = e.directory_page(h, 1, 0).unwrap();
    assert_eq!(repeated[0].cookie, saved);
    assert_eq!(e.directory_page(h, 1, saved).unwrap()[0].cookie, saved + 1);
    let old_name = repeated[0].name.clone();
    e.unlink(1, &old_name, false).unwrap();
    assert_eq!(e.directory_page(h, 1, saved).unwrap()[0].cookie, saved + 1);
    let mut q =
        e.db.prepare(&format!("EXPLAIN QUERY PLAN {}", directory::QUERY))
            .unwrap();
    let plan = q
        .query_map(rusqlite::params![1, 2], |r| r.get::<_, String>(3))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap()
        .join("\n");
    assert!(plan.contains("names_cookie"), "{plan}");
    assert!(!plan.contains("TEMP B-TREE"), "{plan}");
    drop(q);
    e.close(h, 1).unwrap();
    drop(e);
    std::fs::remove_dir_all(p).unwrap();
}
#[test]
fn maximum_names_byte_bound_handle_type_range_and_cookie_replacement() {
    let (p, mut e) = fresh();
    for i in 0..80 {
        let name = format!("{i:03}{}", "x".repeat(252));
        e.create_node(1, name.as_bytes(), 1, 0o644).unwrap();
    }
    let h = e.open(1, libc::O_RDONLY).unwrap();
    let page = e.directory_page(h, 1, 0).unwrap();
    assert!(page.len() < PAGE_ROWS);
    assert!(page.iter().map(|r| r.name.len() + 17).sum::<usize>() <= PAGE_BYTES);
    assert_eq!(e.directory_page(h, 1, -1).unwrap_err(), "EINVAL");
    assert_eq!(e.directory_page(h, 1, i64::MAX).unwrap_err(), "EINVAL");
    assert_eq!(e.directory_page(h + 100, 1, 0).unwrap_err(), "EBADF");
    let file = e.create_node(1, b"plain", 1, 0o644).unwrap().id;
    let fh = e.open(file, libc::O_RDONLY).unwrap();
    assert_eq!(e.directory_page(fh, file, 0).unwrap_err(), "ENOTDIR");
    assert_eq!(e.directory_page(fh, 1, 0).unwrap_err(), "EBADF");
    e.close(fh, file).unwrap();
    let a = e.create_node(1, b"a", 1, 0o644).unwrap().id;
    e.create_node(1, b"b", 1, 0o644).unwrap();
    let cookie: i64 =
        e.db.query_row(
            "SELECT cookie FROM names WHERE parent=1 AND name=?1",
            [b"a".as_slice()],
            |r| r.get(0),
        )
        .unwrap();
    e.rename(1, b"a", 1, b"b", false).unwrap();
    let row: (i64, i64) =
        e.db.query_row(
            "SELECT ino,cookie FROM names WHERE parent=1 AND name=?1",
            [b"b".as_slice()],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(row, (a, cookie));
    let before = e.next;
    e.db.execute(
        "UPDATE counters SET value=9223372036854775807 WHERE name='directory'",
        [],
    )
    .unwrap();
    assert_eq!(
        e.create_node(1, b"overflow", 1, 0o644).err().unwrap(),
        "ENOSPC"
    );
    assert_eq!(e.next, before);
    assert!(e.lookup(1, b"overflow").unwrap().is_none());
    e.close(h, 1).unwrap();
    drop(e);
    std::fs::remove_dir_all(p).unwrap();
}
