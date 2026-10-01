//! Real backing files/SQLite references; no fake allocator or product hooks.
use phase6_live_probe::engine::Engine;
fn fresh() -> (std::path::PathBuf, Engine) {
    let p = std::env::temp_dir().join(format!(
        "p6-retire-{}",
        phase6_live_probe::minio::hex(&layerfs_sandbox::random::<16>().unwrap())
    ));
    std::fs::create_dir(&p).unwrap();
    let e = Engine::create(&p, 2).unwrap();
    (p, e)
}
fn files(e: &Engine) -> usize {
    std::fs::read_dir(&e.sources).unwrap().count()
}
#[test]
fn repeated_overwrite_split_truncate_regrow_preserves_only_live_sources() {
    let (p, mut e) = fresh();
    let id = e.create_node(1, b"file", 1, 0o644).unwrap().id;
    e.write(id, 0, &[b'A'; 4096]).unwrap();
    for _ in 0..100 {
        e.write(id, 17, b"abcdef").unwrap();
        assert_eq!(files(&e), 2);
    }
    let refs: Vec<(i64, i64)> =
        e.db.prepare("SELECT id,refs FROM sources ORDER BY id")
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
    assert_eq!(refs.iter().map(|r| r.1).sum::<i64>(), 3);
    assert_eq!(refs.len(), 2);
    assert!(!e.retirement_pending().unwrap());
    let mut out = [0; 4096];
    e.read(id, 0, &mut out).unwrap();
    assert_eq!(&out[..17], &[b'A'; 17]);
    assert_eq!(&out[17..23], b"abcdef");
    assert_eq!(&out[23..], &[b'A'; 4073]);
    e.truncate(id, 19).unwrap();
    e.truncate(id, 4096).unwrap();
    e.read(id, 0, &mut out).unwrap();
    assert_eq!(&out[17..19], b"ab");
    assert!(out[19..].iter().all(|b| *b == 0));
    e.truncate(id, 0).unwrap();
    assert_eq!(files(&e), 0);
    assert!(!e.retirement_pending().unwrap());
    assert!(e.retirement_work.get().peak_rows <= 64);
    drop(e);
    std::fs::remove_dir_all(p).unwrap();
}
#[test]
fn truncate_retirement_is_batched_and_open_unlinked_victim_remains_readable() {
    let (p, mut e) = fresh();
    let id = e.create_node(1, b"file", 1, 0o644).unwrap().id;
    for i in 0..130 {
        e.write(id, i, b"Z").unwrap();
    }
    assert_eq!(files(&e), 130);
    e.truncate(id, 0).unwrap();
    assert_eq!(files(&e), 66);
    assert!(e.retirement_pending().unwrap());
    e.drain_sources().unwrap();
    assert_eq!(files(&e), 0);
    assert!(!e.retirement_pending().unwrap());
    assert_eq!(e.retirement_work.get().peak_rows, 64);
    let victim = e.create_node(1, b"victim", 1, 0o644).unwrap().id;
    e.write(victim, 0, b"old").unwrap();
    let h = e.open(victim, libc::O_RDWR).unwrap();
    e.unlink(1, b"victim", false).unwrap();
    e.drain_sources().unwrap();
    assert_eq!(files(&e), 1);
    let mut b = [0; 3];
    e.read(victim, 0, &mut b).unwrap();
    assert_eq!(&b, b"old");
    e.write(victim, 0, b"new").unwrap();
    assert_eq!(files(&e), 1);
    e.read(victim, 0, &mut b).unwrap();
    assert_eq!(&b, b"new");
    e.close(h, victim).unwrap();
    drop(e);
    std::fs::remove_dir_all(p).unwrap();
}
#[test]
fn refused_write_has_no_source_effect_and_missing_retirement_file_fails_explicitly() {
    let (p, mut e) = fresh();
    let id = e.create_node(1, b"file", 1, 0o644).unwrap().id;
    e.write(id, 0, b"safe").unwrap();
    let revision = e.revision;
    assert!(e.write(id, -1, b"bad").is_err());
    assert!(e.write(id, 0, &vec![0; 128 * 1024 + 1]).is_err());
    assert_eq!(files(&e), 1);
    assert_eq!(e.revision, revision);
    // Actual externally missing unreferenced file is a provider failure, not a hook.
    for i in 0..130 {
        e.write(id, 100 + i, b"X").unwrap();
    }
    e.truncate(id, 4).unwrap();
    let source: i64 =
        e.db.query_row(
            "SELECT source FROM source_retirement ORDER BY source LIMIT 1",
            [],
            |r| r.get(0),
        )
        .unwrap();
    std::fs::remove_file(e.sources.join(source.to_string())).unwrap();
    assert!(e
        .retire_sources()
        .unwrap_err()
        .contains("source retirement file"));
    let mut b = [0; 4];
    e.read(id, 0, &mut b).unwrap();
    assert_eq!(&b, b"safe");
    assert!(e.retirement_pending().unwrap());
    let revision = e.revision;
    assert!(e.write(id, 0, b"lost").unwrap_err().contains("quarantined"));
    assert!(e.truncate(id, 0).unwrap_err().contains("quarantined"));
    assert!(e.drain_sources().unwrap_err().contains("quarantined"));
    assert_eq!(e.revision, revision);
    e.read(id, 0, &mut b).unwrap();
    assert_eq!(&b, b"safe");
    drop(e);
    std::fs::remove_dir_all(p).unwrap();
}
