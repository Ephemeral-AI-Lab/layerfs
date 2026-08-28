use super::*;

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
