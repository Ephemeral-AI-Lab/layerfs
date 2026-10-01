//! Real SQL bounded deletion and installation failure barriers.
use phase6_live_probe::{
    engine::Engine,
    sql_windows::{self, Table},
};
#[test]
fn indexed_clear_uses64_row_mutations_on_actual_sqlite() {
    let db = rusqlite::Connection::open_in_memory().unwrap();
    db.execute_batch("CREATE TABLE prepared(id INTEGER PRIMARY KEY,body BLOB)")
        .unwrap();
    for i in 0..4097 {
        db.execute("INSERT INTO prepared VALUES(?1,zeroblob(73))", [i])
            .unwrap();
    }
    assert_eq!(sql_windows::clear(&db, Table::Prepared).unwrap(), 65);
    let n: i64 = db
        .query_row("SELECT count(*) FROM prepared", [], |r| r.get(0))
        .unwrap();
    assert_eq!(n, 0);
}
#[test]
fn partial_install_quarantines_and_preserves_readable_accepted_bytes() {
    let p = std::env::temp_dir().join(format!(
        "p6-install-{}",
        phase6_live_probe::minio::hex(&layerfs_sandbox::random::<16>().unwrap())
    ));
    std::fs::create_dir(&p).unwrap();
    let mut e = Engine::create(&p, 2).unwrap();
    let first = e.create_node(1, b"first", 2, 0o755).unwrap().id;
    let id = e.create_node(1, b"data", 1, 0o644).unwrap().id;
    e.write(id, 0, b"safe").unwrap();
    // Actual malformed captured row: first row installs, second refuses before adoption.
    e.db.execute("INSERT INTO prepared VALUES(?1,2,NULL,NULL,NULL)", [first])
        .unwrap();
    e.db.execute("INSERT INTO prepared VALUES(?1,1,x'01',NULL,NULL)", [id])
        .unwrap();
    assert!(e.install_prepared().unwrap_err().contains("root width"));
    assert!(e.node(first).unwrap().published);
    assert!(e.source_ready().unwrap_err().contains("quarantined"));
    let revision = e.revision;
    assert!(e.write(id, 0, b"lost").is_err());
    assert_eq!(e.revision, revision);
    let mut bytes = [0; 4];
    e.read(id, 0, &mut bytes).unwrap();
    assert_eq!(&bytes, b"safe");
    let pending: i64 =
        e.db.query_row("SELECT count(*) FROM prepared", [], |r| r.get(0))
            .unwrap();
    assert_eq!(pending, 1);
    assert!(e.install_prepared().is_err());
    drop(e);
    std::fs::remove_dir_all(p).unwrap();
}
