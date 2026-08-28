use super::*;

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
