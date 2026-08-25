use layerfs_core::inode::InodeId;
use layerfs_core::namespace::NamespaceRootV1;
use layerfs_core::namespace_codec::{encode_namespace_root, profile_id};
use layerfs_core::{encode_bytes_object, ObjectId};
use layerfs_engine::integrity::IntegrityMode;
use layerfs_engine::{Engine, EngineError};
use rusqlite::{params, Connection};
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn trusted_ref_with_missing_inode_table_never_becomes_verified_authority() {
    let path = std::env::temp_dir().join(format!(
        "layerfs-integrity-test-{}-{}.sqlite",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let corrupt_id;
    {
        let engine = Engine::open_with_mode(&path, IntegrityMode::TrustedLocalDev).unwrap();
        let mut publication = engine.begin_publication(None, "main").unwrap();
        let unrelated = encode_bytes_object(b"authenticated while admitted").unwrap();
        corrupt_id = publication.put_object(&unrelated).unwrap();
        let root = encode_namespace_root(NamespaceRootV1 {
            profile_id: profile_id(),
            root_directory_inode: InodeId::allocate([6; 32], 0),
            inode_table_root: ObjectId::for_bytes(b"table"),
        })
        .unwrap();
        publication.publish_namespace(&root).unwrap();
    }
    let raw = Connection::open(&path).unwrap();
    raw.execute("UPDATE layerfs_objects SET canonical_bytes = zeroblob(canonical_length) WHERE object_id = ?1", params![corrupt_id.as_bytes().as_slice()]).unwrap();
    drop(raw);
    {
        let trusted = Engine::open_with_mode(&path, IntegrityMode::TrustedLocalDev).unwrap();
        assert!(matches!(
            trusted.load_object(corrupt_id),
            Err(EngineError::MalformedObject { .. }) | Err(EngineError::IdentityMismatch { .. })
        ));
    }
    assert!(matches!(
        Engine::open(&path),
        Err(EngineError::MissingObject(_))
    ));
    let _ = fs::remove_file(path);
}

#[test]
fn commit_busy_rolls_back_primary_before_reconciliation_and_engine_reuse() {
    let path = std::env::temp_dir().join(format!(
        "layerfs-commit-busy-{}-{}.sqlite",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let engine = Engine::open_with_mode(&path, IntegrityMode::TrustedLocalDev).unwrap();
    let root = |table: &[u8]| {
        encode_namespace_root(NamespaceRootV1 {
            profile_id: profile_id(),
            root_directory_inode: InodeId::allocate([7; 32], 0),
            inode_table_root: ObjectId::for_bytes(table),
        })
        .unwrap()
    };
    let first = engine
        .begin_publication(None, "main")
        .unwrap()
        .publish_namespace(&root(b"first"))
        .unwrap();

    let reader = Connection::open(&path).unwrap();
    reader.execute_batch("BEGIN").unwrap();
    reader
        .query_row(
            "SELECT generation FROM layerfs_refs WHERE name='main'",
            [],
            |row| row.get::<_, i64>(0),
        )
        .unwrap();

    let error = engine
        .begin_publication(Some(&first), "main")
        .unwrap()
        .publish_namespace(&root(b"blocked"))
        .unwrap_err();
    assert!(matches!(
        error,
        EngineError::Sqlite {
            kind: layerfs_engine::SqliteErrorKind::Busy,
            ..
        }
    ));
    assert_eq!(engine.read_ref("main").unwrap(), Some(first.clone()));

    reader.execute_batch("ROLLBACK").unwrap();
    let second = engine
        .begin_publication(Some(&first), "main")
        .unwrap()
        .publish_namespace(&root(b"second"))
        .unwrap();
    assert_eq!(engine.read_ref("main").unwrap(), Some(second));
    drop(reader);
    drop(engine);
    let _ = fs::remove_file(path);
}

#[test]
fn live_verified_handle_rechecks_history_written_by_trusted_handle() {
    let path = std::env::temp_dir().join(format!(
        "layerfs-live-trust-{}-{}.sqlite",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let verified = Engine::open(&path).unwrap();
    let trusted = Engine::open_with_mode(&path, IntegrityMode::TrustedLocalDev).unwrap();
    let root = encode_namespace_root(NamespaceRootV1 {
        profile_id: profile_id(),
        root_directory_inode: InodeId::allocate([0x51; 32], 0),
        inode_table_root: ObjectId::for_bytes(b"concurrently missing table"),
    })
    .unwrap();
    trusted
        .begin_publication(None, "main")
        .unwrap()
        .publish_namespace(&root)
        .unwrap();
    verified.reset_counters().unwrap();
    assert!(matches!(
        verified.begin_publication(None, "probe"),
        Err(EngineError::MissingObject(_))
    ));
    let failed_writer = verified.counters().unwrap();
    assert_eq!(failed_writer.transactions_started, 1);
    assert_eq!(failed_writer.transactions_committed, 0);
    assert_eq!(failed_writer.transactions_rolled_back, 1);
    assert_eq!(failed_writer.publication_transactions_started, 1);
    assert_eq!(failed_writer.publication_transactions_rolled_back, 1);
    assert_eq!(failed_writer.publication_commits, 0);
    verified.reset_counters().unwrap();
    assert!(matches!(
        verified.read_ref("main"),
        Err(EngineError::MissingObject(_))
    ));
    drop(trusted);
    drop(verified);
    let _ = fs::remove_file(path);
}

#[test]
fn initial_verified_scrub_serializes_against_trusted_writer() {
    let path = std::env::temp_dir().join(format!(
        "layerfs-initial-scrub-{}-{}.sqlite",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let trusted = Engine::open_with_mode(&path, IntegrityMode::TrustedLocalDev).unwrap();
    let publication = trusted.begin_publication(None, "main").unwrap();
    assert!(matches!(
        Engine::open(&path),
        Err(EngineError::Sqlite {
            kind: layerfs_engine::SqliteErrorKind::Busy | layerfs_engine::SqliteErrorKind::Locked,
            ..
        })
    ));
    drop(publication);
    drop(Engine::open(&path).unwrap());
    drop(trusted);
    let _ = fs::remove_file(path);
}

#[test]
fn successful_live_scrub_clears_history_once_durably() {
    let path = std::env::temp_dir().join(format!(
        "layerfs-one-live-scrub-{}-{}.sqlite",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let verified = Engine::open(&path).unwrap();
    Connection::open(&path)
        .unwrap()
        .execute(
            "UPDATE layerfs_authority SET trusted_history = 1 WHERE authority_id = 1",
            [],
        )
        .unwrap();

    assert_eq!(verified.read_ref("main").unwrap(), None);
    let first = verified.counters().unwrap();
    assert_eq!(first.retained_union_scrubs, 1);
    assert_eq!(first.integrity_transactions_started, 2);
    assert_eq!(first.integrity_transactions_committed, 1);
    assert_eq!(first.integrity_transactions_rolled_back, 1);
    assert_eq!(first.integrity_statements, 9);
    assert_eq!(
        Connection::open(&path)
            .unwrap()
            .query_row(
                "SELECT trusted_history FROM layerfs_authority WHERE authority_id = 1",
                [],
                |row| row.get::<_, i64>(0),
            )
            .unwrap(),
        0
    );
    assert_eq!(verified.read_ref("main").unwrap(), None);
    let second = verified.counters().unwrap();
    assert_eq!(second.retained_union_scrubs, 1);
    assert_eq!(second.integrity_transactions_started, 3);
    assert_eq!(second.integrity_transactions_committed, 1);
    assert_eq!(second.integrity_transactions_rolled_back, 2);
    assert_eq!(second.integrity_statements, first.integrity_statements + 4);
    drop(verified);
    fs::remove_file(path).unwrap();
}

#[test]
fn verified_reopen_rejects_invalid_ref_rows_before_closure() {
    for (label, tamper) in [
        ("name", "UPDATE layerfs_refs SET name = ''"),
        ("generation", "UPDATE layerfs_refs SET generation = -1"),
        ("membership", "DELETE FROM layerfs_retained_roots"),
    ] {
        let path = std::env::temp_dir().join(format!(
            "layerfs-ref-tamper-{label}-{}-{}.sqlite",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let engine = Engine::open_with_mode(&path, IntegrityMode::TrustedLocalDev).unwrap();
        let root = encode_namespace_root(NamespaceRootV1 {
            profile_id: profile_id(),
            root_directory_inode: InodeId::allocate([0x88; 32], 0),
            inode_table_root: ObjectId::for_bytes(b"unused invalid table"),
        })
        .unwrap();
        engine
            .begin_publication(None, "main")
            .unwrap()
            .publish_namespace(&root)
            .unwrap();
        drop(engine);
        Connection::open(&path)
            .unwrap()
            .execute(tamper, [])
            .unwrap();
        let result = Engine::open(&path);
        match label {
            "name" | "generation" => {
                assert!(matches!(result, Err(EngineError::InvalidRecord(_))))
            }
            "membership" => assert!(matches!(result, Err(EngineError::MissingRoot(_)))),
            _ => unreachable!(),
        }
        fs::remove_file(path).unwrap();
    }
}
