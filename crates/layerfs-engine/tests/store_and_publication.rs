use layerfs_core::content::extent::{ExtentNodeV3, ExtentSliceV3, FileStateV3};
use layerfs_core::content::extent_codec::{
    encode_file_state, encode_node, profile_id as file_profile_id,
};
use layerfs_core::content::rope::{build, read_range};
use layerfs_core::inode::{
    inode_table_from_root, inode_table_upsert, InodeId, InodeKind, InodeRecordV1,
};
use layerfs_core::metadata::{build_metadata_tree, MetadataEntryV1, MetadataKey};
use layerfs_core::namespace::{directory_insert, empty_directory, NamespaceRootV1};
use layerfs_core::namespace_codec::{encode_inode_record, encode_namespace_root, profile_id};
use layerfs_core::{encode_bytes_object, CanonicalName, ObjectId};
use layerfs_engine::integrity::IntegrityMode;
use layerfs_engine::refs::RefState;
use layerfs_engine::{Engine, EngineError};
use rusqlite::{params, Connection};
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn trusted_borrowed_load_and_ordered_batch_skip_identity_authentication() {
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
    assert_eq!(one.fetched_rows, 0);
    assert_eq!(one.fetched_row_authentication_passes, 0);
    assert_eq!(one.fetched_row_role_decode_passes, 0);

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
    assert_eq!(batch.fetched_rows, 3);
    assert_eq!(batch.fetched_row_authentication_passes, 0);
    assert_eq!(batch.fetched_row_role_decode_passes, 3);
    assert_eq!(batch.identity_authentication_ns, 0);
    assert_eq!(batch.payload_batch_queries, 1);
    assert_eq!(batch.payload_batch_references, 3);
    assert_eq!(batch.payload_batch_maximum, 3);

    assert!(matches!(
        engine.for_each_authenticated_payload_batch(&[ObjectId::for_bytes(b"missing")], |_, _| Ok(
            ()
        )),
        Err(EngineError::MissingObject(_))
    ));
    assert!(matches!(
        engine.for_each_authenticated_payload_batch(
            &[first_id, ObjectId::for_bytes(b"missing-middle"), second_id],
            |_, _| Ok(())
        ),
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
fn trusted_reads_are_weaker_but_incumbent_writes_and_verified_reads_authenticate() {
    let path = std::env::temp_dir().join(format!(
        "layerfs-trusted-read-boundary-{}-{}.sqlite",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let engine = Engine::open_with_mode(&path, IntegrityMode::TrustedLocalDev).unwrap();
    let original = encode_bytes_object(b"first").unwrap();
    let substituted = encode_bytes_object(b"other").unwrap();
    let original_id = ObjectId::for_bytes(&original);
    let mut publication = engine.begin_publication(None, "main").unwrap();
    publication.put_object(&original).unwrap();
    let state = publication
        .publish_namespace(
            &encode_namespace_root(NamespaceRootV1 {
                profile_id: profile_id(),
                root_directory_inode: InodeId::allocate([0x91; 32], 0),
                inode_table_root: ObjectId::for_bytes(b"unused table"),
            })
            .unwrap(),
        )
        .unwrap();
    Connection::open(&path)
        .unwrap()
        .execute(
            "UPDATE layerfs_objects SET canonical_bytes = ?1 WHERE object_id = ?2",
            params![&substituted, original_id.as_bytes().as_slice()],
        )
        .unwrap();

    engine.reset_counters().unwrap();
    assert_eq!(
        engine.load_object(original_id).unwrap().canonical_bytes,
        substituted
    );
    let read = engine.counters().unwrap();
    assert_eq!(read.objects_validated, 1);
    assert_eq!(read.fetched_row_authentication_passes, 0);
    assert_eq!(read.identity_authentication_ns, 0);

    let mut publication = engine.begin_publication(Some(&state), "main").unwrap();
    assert!(matches!(
        publication.put_object(&original),
        Err(EngineError::MalformedObject { .. })
    ));
    drop(publication);
    drop(engine);

    Connection::open(&path)
        .unwrap()
        .execute(
            "UPDATE layerfs_authority SET trusted_history = 0 WHERE authority_id = 1",
            [],
        )
        .unwrap();
    let verified = Engine::open(&path).unwrap();
    assert!(matches!(
        verified.load_object(original_id),
        Err(EngineError::MalformedObject { .. })
    ));
    drop(verified);
    fs::remove_file(path).unwrap();
}

#[test]
fn trusted_valid_substitution_is_rejected_by_verified_retained_union_scrub() {
    let base = std::env::temp_dir().join(format!(
        "layerfs-reachable-substitution-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&base).unwrap();
    let path = base.join("store.sqlite");
    let original = b"reachable-original";
    let substituted = b"reachable-impostor";
    assert_eq!(original.len(), substituted.len());
    let original_canonical = encode_bytes_object(original).unwrap();
    let substituted_canonical = encode_bytes_object(substituted).unwrap();
    let payload_id = ObjectId::for_bytes(&original_canonical);
    assert_ne!(payload_id, ObjectId::for_bytes(&substituted_canonical));

    let engine = Engine::open_with_mode(&path, IntegrityMode::TrustedLocalDev).unwrap();
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
    let (content, _) = build(&mut publication, original.as_slice()).unwrap();
    let root_inode = InodeId::allocate([0xa7; 32], 0);
    let file_inode = InodeId::allocate([0xa7; 32], 1);
    let file_record = publication
        .put_object(
            &encode_inode_record(InodeRecordV1 {
                kind: InodeKind::RegularFile,
                namespace_ref_count: 1,
                content_root: content.0,
                metadata_root: metadata,
            })
            .unwrap(),
        )
        .unwrap();
    let directory = empty_directory(&mut publication).unwrap();
    let directory = directory_insert(
        &mut publication,
        directory,
        CanonicalName::new("payload.bin").unwrap(),
        file_inode,
    )
    .unwrap()
    .0;
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
    let table = inode_table_from_root(&mut publication, root_inode, root_record).unwrap();
    let table = inode_table_upsert(&mut publication, table, file_inode, file_record)
        .unwrap()
        .0;
    publication
        .publish_namespace(
            &encode_namespace_root(NamespaceRootV1 {
                profile_id: profile_id(),
                root_directory_inode: root_inode,
                inode_table_root: table.0,
            })
            .unwrap(),
        )
        .unwrap();
    drop(engine);

    let verified = Engine::open(&path).unwrap();
    assert_eq!(verified.counters().unwrap().retained_union_scrubs, 1);
    drop(verified);

    let raw = Connection::open(&path).unwrap();
    assert_eq!(
        raw.execute(
            "UPDATE layerfs_objects SET canonical_bytes = ?1 WHERE object_id = ?2",
            params![&substituted_canonical, payload_id.as_bytes().as_slice()],
        )
        .unwrap(),
        1
    );
    drop(raw);
    let trusted = Engine::open_with_mode(&path, IntegrityMode::TrustedLocalDev).unwrap();
    assert_eq!(
        trusted.load_object(payload_id).unwrap().canonical_bytes,
        substituted_canonical
    );
    assert_eq!(
        trusted
            .counters()
            .unwrap()
            .fetched_row_authentication_passes,
        0
    );
    drop(trusted);

    let error = match Engine::open(&path) {
        Ok(_) => panic!("Verified scrub admitted a substituted reachable object"),
        Err(error) => error,
    };
    assert!(matches!(
        error,
        EngineError::MalformedObject { .. } | EngineError::IdentityMismatch { .. }
    ));
    assert_eq!(
        Connection::open(&path)
            .unwrap()
            .query_row(
                "SELECT trusted_history FROM layerfs_authority WHERE authority_id = 1",
                [],
                |row| row.get::<_, i64>(0),
            )
            .unwrap(),
        1
    );
    let mut entries = fs::read_dir(&base)
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect::<Vec<_>>();
    entries.sort();
    assert_eq!(entries, vec![std::ffi::OsString::from("store.sqlite")]);
    fs::remove_dir_all(base).unwrap();
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
    let committed = engine.counters().unwrap();
    assert_eq!(committed.transactions_committed, 1);
    assert_eq!(committed.publication_transactions_started, 1);
    assert_eq!(committed.publication_transactions_rolled_back, 0);
    assert_eq!(committed.publication_commits, 1);
    assert_eq!(committed.statements, 8);
    assert_eq!(committed.publication_statements, committed.statements);
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
    let no_op_counters = engine.counters().unwrap();
    assert_eq!(no_op_counters.transactions_committed, 0);
    assert_eq!(no_op_counters.publication_transactions_started, 1);
    assert_eq!(no_op_counters.publication_transactions_rolled_back, 1);
    assert_eq!(no_op_counters.publication_commits, 0);
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
    let file_inode = InodeId::allocate([0x44; 32], 1);
    let mut expected = None;
    let mut roots = Vec::new();
    for serial in 0..1_000_u64 {
        let payload = serial.to_be_bytes();
        let mut publication = engine.begin_publication(expected.as_ref(), "main").unwrap();
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
        let (content, _) = build(&mut publication, payload.as_slice()).unwrap();
        let file_record = publication
            .put_object(
                &encode_inode_record(InodeRecordV1 {
                    kind: InodeKind::RegularFile,
                    namespace_ref_count: 1,
                    content_root: content.0,
                    metadata_root: metadata,
                })
                .unwrap(),
            )
            .unwrap();
        let directory = empty_directory(&mut publication).unwrap();
        let directory = directory_insert(
            &mut publication,
            directory,
            CanonicalName::new("entry").unwrap(),
            file_inode,
        )
        .unwrap()
        .0;
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
        let table = inode_table_from_root(&mut publication, root_inode, root_record).unwrap();
        let table = inode_table_upsert(&mut publication, table, file_inode, file_record)
            .unwrap()
            .0;
        let canonical = encode_namespace_root(NamespaceRootV1 {
            profile_id: profile_id(),
            root_directory_inode: root_inode,
            inode_table_root: table.0,
        })
        .unwrap();
        let next = publication.publish_namespace(&canonical).unwrap();
        roots.push((next.clone(), canonical, content, payload));
        expected = Some(next);
    }
    drop(engine);
    let reopened = Engine::open(&path).unwrap();
    let scrub = reopened.counters().unwrap();
    assert_eq!(scrub.retained_union_scrubs, 1);
    assert!(scrub.fetched_rows > 0);
    assert_eq!(scrub.fetched_rows, scrub.fetched_row_authentication_passes);
    assert_eq!(scrub.fetched_rows, scrub.fetched_row_role_decode_passes);
    assert_eq!(reopened.read_ref("main").unwrap(), expected);
    assert_eq!(reopened.retained_roots().unwrap().len(), 1_000);
    for index in [0, 499, 999] {
        let (state, canonical, content, payload) = &roots[index];
        assert_eq!(
            reopened.load_object(state.root).unwrap().canonical_bytes,
            *canonical
        );
        let mut observed = Vec::new();
        read_range(&reopened, *content, 0..payload.len() as u64, &mut observed).unwrap();
        assert_eq!(observed, *payload);
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
fn verified_publication_counts_every_integrity_fetch_authentication_and_role_decode() {
    let path = std::env::temp_dir().join(format!(
        "layerfs-verified-accounting-{}-{}.sqlite",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let engine = Engine::open(&path).unwrap();
    engine.reset_counters().unwrap();
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
    let root_inode = InodeId::allocate([0x74; 32], 0);
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
    publication.publish_namespace(&namespace).unwrap();

    let counters = engine.counters().unwrap();
    assert_eq!(counters.transactions_started, 1);
    assert_eq!(counters.transactions_committed, 1);
    assert_eq!(counters.publication_transactions_started, 1);
    assert_eq!(counters.publication_transactions_rolled_back, 0);
    assert_eq!(counters.publication_commits, 1);
    assert_eq!(counters.integrity_transactions_started, 0);
    assert_eq!(counters.integrity_transactions_committed, 0);
    assert_eq!(counters.integrity_transactions_rolled_back, 0);
    assert_eq!(counters.integrity_statements, 0);
    assert!(counters.fetched_rows > 0);
    assert_eq!(
        counters.fetched_rows,
        counters.fetched_row_authentication_passes
    );
    assert_eq!(
        counters.fetched_rows,
        counters.fetched_row_role_decode_passes
    );
    assert_eq!(counters.publication_closure_passes, 1);
    assert_eq!(counters.namespace_graph_verification_passes, 1);
    assert_eq!(counters.scratch_tables, 2);
    assert!(counters.scratch_statements > 0);
    assert!(counters.scratch_rows > 0);
    assert!(counters.scratch_high_water_bytes > 0);
    drop(engine);
    fs::remove_file(path).unwrap();
}

#[test]
fn verified_open_accounts_schema_profile_authority_and_admission_transaction() {
    let path = std::env::temp_dir().join(format!(
        "layerfs-admission-accounting-{}-{}.sqlite",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));

    let fresh = Engine::open(&path).unwrap();
    let counters = fresh.counters().unwrap();
    assert_eq!(counters.admission_transactions_started, 1);
    assert_eq!(counters.admission_transactions_committed, 1);
    assert_eq!(counters.admission_transactions_rolled_back, 0);
    assert_eq!(counters.admission_statements, 29);
    assert_eq!(counters.store_id_queries, 1);
    assert_eq!(counters.transactions_started, 0);
    assert_eq!(counters.publication_transactions_started, 0);
    assert_eq!(counters.publication_commits, 0);
    drop(fresh);

    let reopened = Engine::open(&path).unwrap();
    let counters = reopened.counters().unwrap();
    assert_eq!(counters.admission_transactions_started, 1);
    assert_eq!(counters.admission_transactions_committed, 1);
    assert_eq!(counters.admission_transactions_rolled_back, 0);
    assert_eq!(counters.admission_statements, 35);
    assert_eq!(counters.store_id_queries, 1);
    assert_eq!(counters.transactions_started, 0);
    assert_eq!(counters.publication_transactions_started, 0);
    assert_eq!(counters.publication_commits, 0);
    drop(reopened);

    fs::remove_file(path).unwrap();
}

#[test]
fn retained_union_reuses_two_scratch_tables_at_five_fifteen_and_thirty_five_roots() {
    let parent = std::env::temp_dir().join(format!(
        "layerfs-retained-union-reuse-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&parent).unwrap();
    let path = parent.join("store.sqlite");
    let trusted = Engine::open_with_mode(&path, IntegrityMode::TrustedLocalDev).unwrap();
    let mut expected: Option<RefState> = None;
    let mut retained = Vec::new();
    let mut shared_content = None;
    let file_inode = InodeId::allocate([0x35; 32], u64::MAX);
    let object_ids = || {
        let connection = Connection::open(&path).unwrap();
        let mut statement = connection
            .prepare("SELECT object_id FROM layerfs_objects ORDER BY object_id")
            .unwrap();
        statement
            .query_map([], |row| row.get::<_, Vec<u8>>(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap()
    };
    let residue = || {
        fs::read_dir(&parent).unwrap().any(|entry| {
            let name = entry.unwrap().file_name();
            let name = name.to_string_lossy();
            name.starts_with(".layerfs-")
                || name.ends_with("-journal")
                || name.ends_with("-wal")
                || name.ends_with("-shm")
        })
    };

    for revision in 0..35_u64 {
        let mut publication = trusted
            .begin_publication(expected.as_ref(), "main")
            .unwrap();
        let (mode, _) = build(&mut publication, 0o755_u32.to_be_bytes().as_slice()).unwrap();
        let mut mtime = Vec::new();
        mtime.extend_from_slice(&(revision as i64).to_be_bytes());
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
        let content = match shared_content {
            Some(content) => content,
            None => {
                let content = build(&mut publication, &[0x5a; 256 * 1024][..]).unwrap().0;
                shared_content = Some(content);
                content
            }
        };
        let file_record = publication
            .put_object(
                &encode_inode_record(InodeRecordV1 {
                    kind: InodeKind::RegularFile,
                    namespace_ref_count: 1,
                    content_root: content.0,
                    metadata_root: metadata,
                })
                .unwrap(),
            )
            .unwrap();
        let directory = empty_directory(&mut publication).unwrap();
        let directory = directory_insert(
            &mut publication,
            directory,
            CanonicalName::new("file").unwrap(),
            file_inode,
        )
        .unwrap()
        .0;
        let root_inode = InodeId::allocate([0x35; 32], revision);
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
        let table = inode_table_upsert(&mut publication, table, file_inode, file_record)
            .unwrap()
            .0;
        let namespace = encode_namespace_root(NamespaceRootV1 {
            profile_id: profile_id(),
            root_directory_inode: root_inode,
            inode_table_root: table.0,
        })
        .unwrap();
        let state = publication.publish_namespace(&namespace).unwrap();
        retained.push(state.root);
        expected = Some(state);

        let root_count = usize::try_from(revision + 1).unwrap();
        if !matches!(root_count, 5 | 15 | 35) {
            continue;
        }
        let objects_before = object_ids();
        let verified = Engine::open(&path).unwrap();
        assert_eq!(verified.retained_roots().unwrap().len(), root_count);
        assert!(retained
            .iter()
            .all(|root| verified.retained_roots().unwrap().contains(root)));
        let counters = verified.counters().unwrap();
        assert_eq!(counters.retained_union_scrubs, 1);
        assert_eq!(counters.scratch_tables, 2);
        assert_eq!(
            counters.namespace_graph_verification_passes,
            root_count as u64
        );
        assert_eq!(
            counters.fetched_rows,
            counters.fetched_row_authentication_passes
        );
        assert_eq!(
            counters.fetched_rows,
            counters.fetched_row_role_decode_passes
        );
        assert_eq!(
            counters.objects_validated,
            counters.fetched_row_authentication_passes
        );
        assert_eq!(counters.transactions_started, 0);
        assert_eq!(counters.transactions_committed, 0);
        assert_eq!(counters.admission_transactions_started, 1);
        assert_eq!(counters.admission_transactions_committed, 1);
        assert_eq!(counters.admission_transactions_rolled_back, 0);
        assert!(counters.admission_statements >= 34);
        assert_eq!(
            counters.integrity_transactions_started,
            root_count as u64 + 1
        );
        assert_eq!(counters.integrity_transactions_committed, 0);
        assert_eq!(
            counters.integrity_transactions_rolled_back,
            root_count as u64 + 1
        );
        assert_eq!(counters.integrity_statements, 4 * (root_count as u64 + 1));
        assert_eq!(counters.retained_roots_validated, root_count as u64);
        assert_eq!(counters.publication_commits, 0);
        assert_eq!(counters.root_verifications, 0);
        assert_eq!(counters.publication_closure_passes, 0);
        assert_eq!(
            counters.scratch_statements,
            match root_count {
                5 => 430,
                15 => 1_140,
                35 => 2_560,
                _ => unreachable!(),
            },
            "retained-root scrub stopped batching payload-summary lookups"
        );
        if root_count == 35 {
            assert!(
                counters.object_bytes_read < 4 * 1024 * 1024,
                "shared payload was redundantly fetched per root: {} bytes",
                counters.object_bytes_read
            );
        }
        assert_eq!(object_ids(), objects_before);
        assert!(!residue());
        drop(verified);
        assert!(!residue());
    }

    drop(trusted);
    fs::remove_file(path).unwrap();
    fs::remove_dir(parent).unwrap();
}

#[test]
fn retained_union_rejects_corrupt_canonical_roles_and_ref_rows_without_residue() {
    let parent = std::env::temp_dir().join(format!(
        "layerfs-retained-union-corruption-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&parent).unwrap();
    let master = parent.join("master.sqlite");
    let trusted = Engine::open_with_mode(&master, IntegrityMode::TrustedLocalDev).unwrap();
    let mut publication = trusted.begin_publication(None, "main").unwrap();
    let mode_bytes = 0o755_u32.to_be_bytes();
    let (mode, _) = build(&mut publication, mode_bytes.as_slice()).unwrap();
    let payload = ObjectId::for_bytes(&encode_bytes_object(&mode_bytes).unwrap());
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
    let root_inode = InodeId::allocate([0xc1; 32], 0);
    let inode_record = publication
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
    let inode_table = inode_table_from_root(&mut publication, root_inode, inode_record)
        .unwrap()
        .0;
    let namespace = encode_namespace_root(NamespaceRootV1 {
        profile_id: profile_id(),
        root_directory_inode: root_inode,
        inode_table_root: inode_table,
    })
    .unwrap();
    let namespace = publication.publish_namespace(&namespace).unwrap().root;
    drop(trusted);

    for (label, object) in [
        ("payload", payload),
        ("namespace", namespace),
        ("inode-table", inode_table),
        ("inode-record", inode_record),
    ] {
        let path = parent.join(format!("{label}.sqlite"));
        fs::copy(&master, &path).unwrap();
        Connection::open(&path)
            .unwrap()
            .execute(
                "UPDATE layerfs_objects SET canonical_bytes = zeroblob(canonical_length)
                 WHERE object_id = ?1",
                params![object.as_bytes().as_slice()],
            )
            .unwrap();
        assert!(matches!(
            Engine::open(&path),
            Err(EngineError::MalformedObject { .. })
                | Err(EngineError::IdentityMismatch { .. })
                | Err(EngineError::Core(_))
        ));
        assert!(!fs::read_dir(&parent).unwrap().any(|entry| entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".layerfs-")));
        fs::remove_file(path).unwrap();
    }

    for (label, sql) in [
        ("ref-name", "UPDATE layerfs_refs SET name = ''"),
        ("ref-generation", "UPDATE layerfs_refs SET generation = -1"),
        ("root-membership", "DELETE FROM layerfs_retained_roots"),
    ] {
        let path = parent.join(format!("{label}.sqlite"));
        fs::copy(&master, &path).unwrap();
        Connection::open(&path).unwrap().execute(sql, []).unwrap();
        assert!(Engine::open(&path).is_err());
        assert!(!fs::read_dir(&parent).unwrap().any(|entry| entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".layerfs-")));
        fs::remove_file(path).unwrap();
    }

    fs::remove_file(master).unwrap();
    fs::remove_dir(parent).unwrap();
}

#[test]
fn retained_union_rejects_bad_link_count_and_unreachable_inode_without_residue() {
    let parent = std::env::temp_dir().join(format!(
        "layerfs-retained-union-graph-faults-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&parent).unwrap();
    for (label, bad_link_count, unreachable, bad_slice) in [
        ("link-count", true, false, false),
        ("unreachable", false, true, false),
        ("payload-slice-bounds", false, false, true),
    ] {
        let path = parent.join(format!("{label}.sqlite"));
        let trusted = Engine::open_with_mode(&path, IntegrityMode::TrustedLocalDev).unwrap();
        let mut publication = trusted.begin_publication(None, "main").unwrap();
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
        let content_root = if bad_slice {
            let payload = publication
                .put_object(&encode_bytes_object(b"x").unwrap())
                .unwrap();
            let mapping = publication
                .put_object(
                    &encode_node(&ExtentNodeV3::Leaf {
                        subtree_logical_bytes: 1,
                        extents: vec![ExtentSliceV3::new(payload, 1, 1).unwrap()],
                    })
                    .unwrap(),
                )
                .unwrap();
            publication
                .put_object(
                    &encode_file_state(FileStateV3 {
                        logical_len: 1,
                        extent_count: 1,
                        tree_level: 0,
                        profile_id: file_profile_id(),
                        mapping_root: mapping,
                    })
                    .unwrap(),
                )
                .unwrap()
        } else {
            build(&mut publication, b"content".as_slice()).unwrap().0 .0
        };
        let root_inode = InodeId::allocate([0xc2; 32], 0);
        let file_inode = InodeId::allocate([0xc2; 32], 1);
        let file_record = publication
            .put_object(
                &encode_inode_record(InodeRecordV1 {
                    kind: InodeKind::RegularFile,
                    namespace_ref_count: if bad_link_count { 2 } else { 1 },
                    content_root,
                    metadata_root: metadata,
                })
                .unwrap(),
            )
            .unwrap();
        let directory = empty_directory(&mut publication).unwrap();
        let directory = directory_insert(
            &mut publication,
            directory,
            CanonicalName::new("file").unwrap(),
            file_inode,
        )
        .unwrap()
        .0;
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
        let table = inode_table_from_root(&mut publication, root_inode, root_record).unwrap();
        let mut table = inode_table_upsert(&mut publication, table, file_inode, file_record)
            .unwrap()
            .0;
        if unreachable {
            let extra_inode = InodeId::allocate([0xc2; 32], 2);
            let extra_record = publication
                .put_object(
                    &encode_inode_record(InodeRecordV1 {
                        kind: InodeKind::RegularFile,
                        namespace_ref_count: 1,
                        content_root,
                        metadata_root: metadata,
                    })
                    .unwrap(),
                )
                .unwrap();
            table = inode_table_upsert(&mut publication, table, extra_inode, extra_record)
                .unwrap()
                .0;
        }
        publication
            .publish_namespace(
                &encode_namespace_root(NamespaceRootV1 {
                    profile_id: profile_id(),
                    root_directory_inode: root_inode,
                    inode_table_root: table.0,
                })
                .unwrap(),
            )
            .unwrap();
        drop(trusted);
        assert!(matches!(Engine::open(&path), Err(EngineError::Core(_))));
        assert!(!fs::read_dir(&parent).unwrap().any(|entry| entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".layerfs-")));
        fs::remove_file(path).unwrap();
    }
    fs::remove_dir(parent).unwrap();
}

#[test]
fn verified_publication_survives_cache_spill_without_reopening_store() {
    let path = std::env::temp_dir().join(format!(
        "layerfs-verified-spill-{}-{}.sqlite",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let engine = Engine::open(&path).unwrap();
    assert_eq!(engine.profile().cache_pages, 1280);
    assert_eq!(engine.profile().cache_spill_pages, 1280);
    let mut state = 0x75a1_5eed_cafe_babe_u64;
    let content_bytes = (0..6 * 1024 * 1024)
        .map(|_| {
            state ^= state << 7;
            state ^= state >> 9;
            state ^= state << 8;
            state as u8
        })
        .collect::<Vec<_>>();
    let aborted_content = {
        let mut publication = engine.begin_publication(None, "aborted").unwrap();
        let (content, _) = build(&mut publication, content_bytes.as_slice()).unwrap();
        assert!(fs::metadata(&path).unwrap().len() > 1024 * 1024);
        content
    };
    assert!(matches!(
        engine.load_object(aborted_content.0),
        Err(EngineError::MissingObject(_))
    ));
    assert_eq!(engine.read_ref("aborted").unwrap(), None);

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
    let (content, _) = build(&mut publication, content_bytes.as_slice()).unwrap();
    let root_inode = InodeId::allocate([0x75; 32], 0);
    let file_inode = InodeId::allocate([0x75; 32], 1);
    let file_record = publication
        .put_object(
            &encode_inode_record(InodeRecordV1 {
                kind: InodeKind::RegularFile,
                namespace_ref_count: 1,
                content_root: content.0,
                metadata_root: metadata,
            })
            .unwrap(),
        )
        .unwrap();
    let directory = empty_directory(&mut publication).unwrap();
    let directory = directory_insert(
        &mut publication,
        directory,
        CanonicalName::new("large").unwrap(),
        file_inode,
    )
    .unwrap()
    .0;
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
    let table = inode_table_from_root(&mut publication, root_inode, root_record).unwrap();
    let table = inode_table_upsert(&mut publication, table, file_inode, file_record)
        .unwrap()
        .0;
    publication
        .publish_namespace(
            &encode_namespace_root(NamespaceRootV1 {
                profile_id: profile_id(),
                root_directory_inode: root_inode,
                inode_table_root: table.0,
            })
            .unwrap(),
        )
        .unwrap();
    assert!(fs::metadata(&path).unwrap().len() > 5 * 1024 * 1024);
    drop(engine);
    let reopened = Engine::open(&path).unwrap();
    let mut observed = Vec::new();
    read_range(
        &reopened,
        content,
        0..content_bytes.len() as u64,
        &mut observed,
    )
    .unwrap();
    assert_eq!(observed, content_bytes);
    drop(reopened);
    fs::remove_file(path).unwrap();
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
