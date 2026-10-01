use phase6_live_probe::engine::Engine;
use std::path::PathBuf;
fn fresh() -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "phase6-engine-{}",
        phase6_live_probe::minio::hex(&layerfs_sandbox::random::<16>().unwrap())
    ));
    std::fs::create_dir(&path).unwrap();
    path
}
#[test]
fn ordinary_final_state_sources_and_handles() {
    let path = fresh();
    let mut engine = Engine::create(&path, 2).unwrap();
    let node = engine.create_node(1, b"data", 1, 0o644).unwrap();
    let handle = engine.open(node.id, libc::O_RDWR).unwrap();
    engine.write(node.id, 0, &[b'A'; 4096]).unwrap();
    engine.write(node.id, 17, b"phase6").unwrap();
    engine.write(node.id, 100, b"SECOND").unwrap();
    let mut expected = vec![b'A'; 4096];
    expected[17..23].copy_from_slice(b"phase6");
    expected[100..106].copy_from_slice(b"SECOND");
    let mut out = vec![0; 4096];
    assert_eq!(engine.read(node.id, 0, &mut out).unwrap(), 4096);
    assert_eq!(out, expected);
    engine.truncate(node.id, 102).unwrap();
    engine.truncate(node.id, 200).unwrap();
    let mut out = vec![1; 300];
    assert_eq!(engine.read(node.id, 0, &mut out).unwrap(), 200);
    assert_eq!(&out[..102], &expected[..102]);
    assert_eq!(&out[102..200], &[0; 98]);
    engine.handle(handle, node.id, true).unwrap();
    engine.close(handle, node.id).unwrap();
    assert!(engine.handle(handle, node.id, false).is_err());
    drop(engine);
    std::fs::remove_dir_all(path).unwrap();
}
#[test]
fn repeated_overwrites_do_not_retain_write_history() {
    let path = fresh();
    let mut engine = Engine::create(&path, 2).unwrap();
    let node = engine.create_node(1, b"data", 1, 0o644).unwrap();
    engine.write(node.id, 0, &[b'A'; 4096]).unwrap();
    for _ in 0..64 {
        engine.write(node.id, 17, b"phase6").unwrap();
    }
    let rows: i64 = engine
        .db
        .query_row(
            "SELECT count(*) FROM extents WHERE ino=?1",
            [node.id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(rows, 3);
    let sources: i64 = engine
        .db
        .query_row("SELECT count(*) FROM sources", [], |r| r.get(0))
        .unwrap();
    assert_eq!(
        sources, 2,
        "only the original split source and latest overwrite remain referenced"
    );
    drop(engine);
    std::fs::remove_dir_all(path).unwrap();
}
#[test]
fn exact_handle_admission_and_refund() {
    let path = fresh();
    let mut engine = Engine::create(&path, 2).unwrap();
    let mut handles = Vec::new();
    for _ in 0..256 {
        handles.push(engine.open(1, libc::O_RDONLY).unwrap());
    }
    assert!(engine.open(1, libc::O_RDONLY).is_err());
    engine.close(handles.pop().unwrap(), 1).unwrap();
    assert!(engine.open(1, libc::O_RDONLY).is_ok());
    drop(engine);
    std::fs::remove_dir_all(path).unwrap();
}
