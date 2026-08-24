use layerfs_core::content::rope::{build, read_range};
use layerfs_core::inode::{
    inode_table_from_root, inode_table_upsert, InodeId, InodeKind, InodeRecordV1,
};
use layerfs_core::metadata::{build_metadata_tree, MetadataEntryV1, MetadataKey};
use layerfs_core::namespace::{empty_directory, NamespaceRootV1};
use layerfs_core::namespace_codec::{encode_inode_record, encode_namespace_root, profile_id};
use layerfs_core::{encode_bytes_object, ObjectId};
use layerfs_engine::integrity::IntegrityMode;
use layerfs_engine::refs::RefState;
use layerfs_engine::{Engine, EngineError};
use rusqlite::{params, Connection};
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn borrowed_object_load_and_ordered_batch_fetch_once_and_authenticate_once_per_occurrence() {
    let path = std::env::temp_dir().join(format!(
        "layerfs-store-test-{}-{}.sqlite",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let engine = Engine::open_with_mode(&path, IntegrityMode::TrustedLocalDev).unwrap();
    let first = encode_bytes_object(b"first").unwrap();
    let second = encode_bytes_object(b"second").unwrap();
    let first_id = ObjectId::for_bytes(&first);
    let second_id = ObjectId::for_bytes(&second);
    let mut publication = engine.begin_publication(None, "objects").unwrap();
    publication.put_object(&first).unwrap();
    publication.put_object(&second).unwrap();
    publication
        .publish_namespace(
            &encode_namespace_root(NamespaceRootV1 {
                profile_id: profile_id(),
                root_directory_inode: InodeId::allocate([1; 32], 0),
                inode_table_root: ObjectId::for_bytes(b"reader-test-table"),
            })
            .unwrap(),
        )
        .unwrap();

    engine.reset_counters().unwrap();
    assert_eq!(engine.load_object(first_id).unwrap().canonical_bytes, first);
    let one = engine.counters().unwrap();
    assert_eq!(one.statements, 1);
    assert_eq!(one.objects_validated, 1);

    engine.reset_counters().unwrap();
    let requested = [second_id, first_id, second_id];
    let mut observed = Vec::new();
    engine
        .for_each_authenticated_payload_batch(&requested, |id, bytes| {
            observed.push((id, bytes.to_vec()));
            Ok(())
        })
        .unwrap();
    assert_eq!(
        observed.iter().map(|entry| entry.0).collect::<Vec<_>>(),
        requested
    );
    let batch = engine.counters().unwrap();
    assert_eq!(batch.statements, 1);
    assert_eq!(batch.objects_validated, 3);

    assert!(matches!(
        engine.for_each_authenticated_payload_batch(&[ObjectId::for_bytes(b"missing")], |_, _| Ok(
            ()
        )),
        Err(EngineError::MissingObject(_))
    ));
    assert_eq!(
        engine.for_each_authenticated_payload_batch(&vec![first_id; 65], |_, _| Ok(())),
        Err(EngineError::InvalidRecord("payload batch exceeds 64"))
    );
    drop(engine);
    let _ = fs::remove_file(path);
}

#[test]
fn canonical_rope_objects_share_the_publication_transaction() {
    let path = std::env::temp_dir().join(format!(
        "layerfs-core-publication-test-{}-{}.sqlite",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let engine = Engine::open_with_mode(&path, IntegrityMode::TrustedLocalDev).unwrap();
    let bytes = vec![0x5a; 200_000];
    let rolled_back_root = {
        let mut publication = engine.begin_publication(None, "aborted").unwrap();
        let (root, _) = build(&mut publication, bytes.as_slice()).unwrap();
        root
    };
    assert!(matches!(
        engine.load_object(rolled_back_root.0),
        Err(EngineError::MissingObject(_))
    ));

    let (file_root, namespace_ref) = {
        let mut publication = engine.begin_publication(None, "main").unwrap();
        let (file_root, _) = build(&mut publication, bytes.as_slice()).unwrap();
        let namespace = encode_namespace_root(NamespaceRootV1 {
            profile_id: profile_id(),
            root_directory_inode: InodeId::allocate([8; 32], 0),
            inode_table_root: ObjectId::for_bytes(b"table"),
        })
        .unwrap();
        let state = publication.publish_namespace(&namespace).unwrap();
        (file_root, state)
    };
    let mut actual = Vec::new();
    read_range(&engine, file_root, 0..bytes.len() as u64, &mut actual).unwrap();
    assert_eq!(actual, bytes);
    assert_eq!(engine.read_ref("main").unwrap().unwrap(), namespace_ref);
    drop(engine);
    let _ = fs::remove_file(path);
}

#[test]
fn publication_is_one_guarded_commit_and_fork_rollback_copy_no_objects() {
    let path = std::env::temp_dir().join(format!(
        "layerfs-publication-test-{}-{}.sqlite",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let engine = Engine::open_with_mode(&path, IntegrityMode::TrustedLocalDev).unwrap();
    let inode_table_a = ObjectId::for_bytes(b"table A");
    let inode_table_b = ObjectId::for_bytes(b"table B");
    let root_inode = InodeId::allocate([4; 32], 0);
    let root_a_bytes = encode_namespace_root(NamespaceRootV1 {
        profile_id: profile_id(),
        root_directory_inode: root_inode,
        inode_table_root: inode_table_a,
    })
    .unwrap();
    let root_b_bytes = encode_namespace_root(NamespaceRootV1 {
        profile_id: profile_id(),
        root_directory_inode: root_inode,
        inode_table_root: inode_table_b,
    })
    .unwrap();

    let mut aborted = engine.begin_publication(None, "aborted").unwrap();
    let allocated = aborted.allocate_inode_id().unwrap();
    drop(aborted);
    let mut retried = engine.begin_publication(None, "aborted").unwrap();
    assert_eq!(
        retried.allocate_inode_id().unwrap(),
        allocated,
        "rollback consumed durable inode serial"
    );
    drop(retried);

    engine.reset_counters().unwrap();
    let a = engine
        .begin_publication(None, "main")
        .unwrap()
        .publish_namespace(&root_a_bytes)
        .unwrap();
    assert_eq!(engine.counters().unwrap().transactions_committed, 1);
    let stale = a.clone();
    let b = engine
        .begin_publication(Some(&a), "main")
        .unwrap()
        .publish_namespace(&root_b_bytes)
        .unwrap();
    assert_eq!(b.generation, 1);
    engine.reset_counters().unwrap();
    let no_op = engine
        .begin_publication(Some(&b), "main")
        .unwrap()
        .publish_namespace(&root_b_bytes)
        .unwrap();
    assert_eq!(no_op, b);
    assert_eq!(engine.counters().unwrap().transactions_committed, 0);
    assert!(matches!(
        engine.begin_publication(Some(&stale), "main"),
        Err(EngineError::PublicationConflict)
    ));

    let fork = engine.fork_ref(&a, "experiment").unwrap();
    assert_eq!(fork.root, a.root);
    let rolled_back = engine.move_ref(&b, a.root).unwrap();
    assert_eq!(rolled_back.root, a.root);
    assert_eq!(engine.read_ref("experiment").unwrap().unwrap(), fork);
    let retained = engine.retained_roots().unwrap();
    assert!(retained.contains(&a.root) && retained.contains(&b.root));
    Connection::open(&path)
        .unwrap()
        .execute(
            "DELETE FROM layerfs_retained_roots WHERE root_id = ?1",
            params![b.root.as_bytes().as_slice()],
        )
        .unwrap();
    assert!(matches!(
        engine.move_ref(&rolled_back, b.root),
        Err(EngineError::MissingRoot(missing)) if missing == b.root
    ));
    assert_eq!(engine.read_ref("main").unwrap(), Some(rolled_back));
    drop(engine);
    let _ = fs::remove_file(path);
}

#[test]
fn one_thousand_tiny_revisions_remain_directly_readable_after_reopen() {
    let path = std::env::temp_dir().join(format!(
        "layerfs-thousand-revisions-{}-{}.sqlite",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let engine = Engine::open_with_mode(&path, IntegrityMode::TrustedLocalDev).unwrap();
    let root_inode = InodeId::allocate([0x44; 32], 0);
    let mut expected = None;
    let mut roots = Vec::new();
    for serial in 0..1_000_u64 {
        let canonical = encode_namespace_root(NamespaceRootV1 {
            profile_id: profile_id(),
            root_directory_inode: root_inode,
            inode_table_root: ObjectId::for_bytes(&serial.to_be_bytes()),
        })
        .unwrap();
        let next = engine
            .begin_publication(expected.as_ref(), "main")
            .unwrap()
            .publish_namespace(&canonical)
            .unwrap();
        roots.push((next.root, canonical));
        expected = Some(next);
    }
    drop(engine);
    let reopened = Engine::open_with_mode(&path, IntegrityMode::TrustedLocalDev).unwrap();
    for (root, canonical) in roots {
        assert_eq!(
            reopened.load_object(root).unwrap().canonical_bytes,
            canonical
        );
    }
    drop(reopened);
    fs::remove_file(path).unwrap();
}

#[test]
fn verified_publication_rejects_missing_reachable_inode_table_before_visibility() {
    let path = std::env::temp_dir().join(format!(
        "layerfs-verified-publication-{}-{}.sqlite",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let engine = Engine::open(&path).unwrap();
    let root = encode_namespace_root(NamespaceRootV1 {
        profile_id: profile_id(),
        root_directory_inode: InodeId::allocate([0x73; 32], 0),
        inode_table_root: ObjectId::for_bytes(b"missing inode table"),
    })
    .unwrap();
    assert!(matches!(
        engine
            .begin_publication(None, "main")
            .unwrap()
            .publish_namespace(&root),
        Err(EngineError::MissingObject(_))
    ));
    assert_eq!(engine.read_ref("main").unwrap(), None);
    drop(engine);
    let _ = fs::remove_file(path);
}

#[test]
fn verified_publication_rejects_all_hashes_correct_unreachable_inode() {
    let path = std::env::temp_dir().join(format!(
        "layerfs-semantic-publication-{}-{}.sqlite",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let engine = Engine::open(&path).unwrap();
    let mut publication = engine.begin_publication(None, "main").unwrap();
    let (mode, _) = build(&mut publication, 0o755_u32.to_be_bytes().as_slice()).unwrap();
    let mut mtime = Vec::new();
    mtime.extend_from_slice(&0_i64.to_be_bytes());
    mtime.extend_from_slice(&0_u32.to_be_bytes());
    let (mtime, _) = build(&mut publication, mtime.as_slice()).unwrap();
    let metadata = build_metadata_tree(
        &mut publication,
        &[
            MetadataEntryV1 {
                key: MetadataKey::new("portable".into(), b"mode".to_vec()).unwrap(),
                value_file_root: mode.0,
            },
            MetadataEntryV1 {
                key: MetadataKey::new("portable".into(), b"mtime".to_vec()).unwrap(),
                value_file_root: mtime.0,
            },
        ],
    )
    .unwrap();
    let directory = empty_directory(&mut publication).unwrap();
    let root_inode = InodeId::allocate([0x33; 32], 0);
    let root_record = publication
        .put_object(
            &encode_inode_record(InodeRecordV1 {
                kind: InodeKind::Directory,
                namespace_ref_count: 0,
                content_root: directory.0,
                metadata_root: metadata,
            })
            .unwrap(),
        )
        .unwrap();
    let mut table = inode_table_from_root(&mut publication, root_inode, root_record).unwrap();
    let extra_inode = InodeId::allocate([0x33; 32], 1);
    let (empty_file, _) = build(&mut publication, b"".as_slice()).unwrap();
    let extra_record = publication
        .put_object(
            &encode_inode_record(InodeRecordV1 {
                kind: InodeKind::RegularFile,
                namespace_ref_count: 1,
                content_root: empty_file.0,
                metadata_root: metadata,
            })
            .unwrap(),
        )
        .unwrap();
    table = inode_table_upsert(&mut publication, table, extra_inode, extra_record)
        .unwrap()
        .0;
    let namespace = encode_namespace_root(NamespaceRootV1 {
        profile_id: profile_id(),
        root_directory_inode: root_inode,
        inode_table_root: table.0,
    })
    .unwrap();
    assert_eq!(
        publication.publish_namespace(&namespace),
        Err(EngineError::Core(layerfs_core::CoreError::InvalidRecord(
            "unreachable inode table entry"
        )))
    );
    assert_eq!(engine.read_ref("main").unwrap(), None);
    drop(engine);
    let _ = fs::remove_file(path);
}

#[test]
fn verified_publication_rejects_present_zero_bsd_flags() {
    let path = std::env::temp_dir().join(format!(
        "layerfs-zero-bsd-flags-{}-{}.sqlite",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let engine = Engine::open(&path).unwrap();
    let mut publication = engine.begin_publication(None, "main").unwrap();
    let (flags, _) = build(&mut publication, 0_u32.to_be_bytes().as_slice()).unwrap();
    let (mode, _) = build(&mut publication, 0o755_u32.to_be_bytes().as_slice()).unwrap();
    let mut mtime = Vec::new();
    mtime.extend_from_slice(&0_i64.to_be_bytes());
    mtime.extend_from_slice(&0_u32.to_be_bytes());
    let (mtime, _) = build(&mut publication, mtime.as_slice()).unwrap();
    let metadata = build_metadata_tree(
        &mut publication,
        &[
            MetadataEntryV1 {
                key: MetadataKey::new("apple.bsd-flags".into(), Vec::new()).unwrap(),
                value_file_root: flags.0,
            },
            MetadataEntryV1 {
                key: MetadataKey::new("portable".into(), b"mode".to_vec()).unwrap(),
                value_file_root: mode.0,
            },
            MetadataEntryV1 {
                key: MetadataKey::new("portable".into(), b"mtime".to_vec()).unwrap(),
                value_file_root: mtime.0,
            },
        ],
    )
    .unwrap();
    let directory = empty_directory(&mut publication).unwrap();
    let root_inode = InodeId::allocate([0x44; 32], 0);
    let record = publication
        .put_object(
            &encode_inode_record(InodeRecordV1 {
                kind: InodeKind::Directory,
                namespace_ref_count: 0,
                content_root: directory.0,
                metadata_root: metadata,
            })
            .unwrap(),
        )
        .unwrap();
    let table = inode_table_from_root(&mut publication, root_inode, record).unwrap();
    let namespace = encode_namespace_root(NamespaceRootV1 {
        profile_id: profile_id(),
        root_directory_inode: root_inode,
        inode_table_root: table.0,
    })
    .unwrap();
    assert_eq!(
        publication.publish_namespace(&namespace),
        Err(EngineError::Core(layerfs_core::CoreError::InvalidRecord(
            "BSD flags"
        )))
    );
    assert_eq!(engine.read_ref("main").unwrap(), None);
    drop(engine);
    fs::remove_file(path).unwrap();
}

#[test]
fn verified_fork_rejects_unretained_orphan_root() {
    let path = std::env::temp_dir().join(format!(
        "layerfs-invalid-fork-{}-{}.sqlite",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let engine = Engine::open(&path).unwrap();
    let canonical = encode_namespace_root(NamespaceRootV1 {
        profile_id: profile_id(),
        root_directory_inode: InodeId::allocate([0x92; 32], 0),
        inode_table_root: ObjectId::for_bytes(b"orphan missing table"),
    })
    .unwrap();
    let root = ObjectId::for_bytes(&canonical);
    Connection::open(&path)
        .unwrap()
        .execute(
            "INSERT INTO layerfs_objects (object_id, kind, canonical_length, canonical_bytes) VALUES (?1, 1, ?2, ?3)",
            params![root.as_bytes().as_slice(), canonical.len() as i64, canonical],
        )
        .unwrap();
    let source = RefState {
        name: "orphan".into(),
        generation: 0,
        root,
    };
    assert!(matches!(
        engine.fork_ref(&source, "invalid-fork"),
        Err(EngineError::MissingRoot(missing)) if missing == root
    ));
    assert_eq!(engine.read_ref("invalid-fork").unwrap(), None);
    drop(engine);
    let _ = fs::remove_file(path);
}
