#[cfg(test)]
mod product_tests {
    use super::*;
    use layerfs_core::inode::{inode_table_from_root, InodeKind, InodeRecordV1};
    use layerfs_core::metadata::{build_metadata_tree, MetadataEntryV1, MetadataKey};
    use layerfs_core::namespace::{empty_directory, NamespaceRootV1};
    use layerfs_core::namespace_codec::{encode_inode_record, encode_namespace_root, profile_id};
    use layerfs_core::object::access::ObjectStore;
    use layerfs_workspace::{
        BranchId, CommitResult, LayerId, LayerStackId, Presentation, WorkingCandidate,
        WorkspacePaths, WorkspaceTicket,
    };

    fn empty_root(working: &WorkingStore) -> ObjectId {
        let mut writer = working.begin_candidate_write().unwrap();
        let inode = writer.allocate_inode_id().unwrap();
        let mode = build(&mut writer, Cursor::new(0o755_u32.to_be_bytes()))
            .unwrap()
            .0;
        let mtime = build(&mut writer, Cursor::new([0_u8; 12])).unwrap().0;
        let metadata = build_metadata_tree(
            &mut writer,
            &[
                MetadataEntryV1 {
                    key: MetadataKey::new("portable".to_owned(), b"mode".to_vec()).unwrap(),
                    value_file_root: mode.0,
                },
                MetadataEntryV1 {
                    key: MetadataKey::new("portable".to_owned(), b"mtime".to_vec()).unwrap(),
                    value_file_root: mtime.0,
                },
            ],
        )
        .unwrap();
        let directory = empty_directory(&mut writer).unwrap();
        let record = writer
            .put(
                &encode_inode_record(InodeRecordV1 {
                    kind: InodeKind::Directory,
                    namespace_ref_count: 0,
                    content_root: directory.0,
                    metadata_root: metadata,
                })
                .unwrap(),
            )
            .unwrap();
        let table = inode_table_from_root(&mut writer, inode, record).unwrap();
        let root = writer
            .put(
                &encode_namespace_root(NamespaceRootV1 {
                    profile_id: profile_id(),
                    root_directory_inode: inode,
                    inode_table_root: table.0,
                })
                .unwrap(),
            )
            .unwrap();
        writer.commit_candidate(root).unwrap()
    }

    #[test]
    fn fsync_is_private_until_exact_operation_commit() {
        let directory = std::env::temp_dir().join(format!(
            "layerfs-mount-product-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&directory).unwrap();
        let working_root = directory.join("working");
        let working = WorkingStore::open(&working_root, IntegrityMode::TrustedLocalDev).unwrap();
        let root = empty_root(&working);
        let stack = working
            .create_layer_stack(
                LayerStackId::from_bytes([0x11; 32]),
                LayerId::from_bytes([0x12; 32]),
                "stack",
                root,
            )
            .unwrap();
        let head = working
            .create_top_level_branch(BranchId::from_bytes([0x13; 32]), Some("main"), stack)
            .unwrap();
        let admission = working.begin_operation(head).unwrap();
        let mut mounted = MountedWorkspace::open(
            &working_root,
            admission,
            IntegrityMode::TrustedLocalDev,
            directory.join("spool"),
        )
        .unwrap();
        working.reset_counters().unwrap();
        let (file, handle) = mounted.create_file(ROOT_NODE, b"file", 0o644).unwrap();
        mounted.write(file.node, handle, 0, b"private").unwrap();
        mounted.release(handle).unwrap();
        let first_candidate = mounted.fsync().unwrap();
        assert_ne!(first_candidate, root);
        assert_eq!(working.branch_head(head.branch_id).unwrap(), Some(head));
        let handle = mounted.open_file(file.node, false).unwrap();
        mounted.write(file.node, handle, 7, b"-latest").unwrap();
        mounted.release(handle).unwrap();
        let candidate = mounted.fsync().unwrap();
        assert_ne!(candidate, first_candidate);
        assert_eq!(working.branch_head(head.branch_id).unwrap(), Some(head));
        drop(mounted);
        let reopened = WorkingStore::open(&working_root, IntegrityMode::TrustedLocalDev).unwrap();
        let recovery = reopened.recoverable_operations(8).unwrap();
        assert_eq!(recovery.len(), 1);
        assert_eq!(recovery[0].operation_id, admission.operation_id);
        assert_eq!(recovery[0].candidate_root, Some(candidate));
        assert_eq!(reopened.branch_head(head.branch_id).unwrap(), Some(head));
        drop(reopened);

        let result = working
            .operation_commit(
                admission,
                WorkingCandidate {
                    operation_id: admission.operation_id,
                    expected_branch_generation: head.generation,
                    base_root: root,
                    candidate_root: candidate,
                    normalized_transition: Vec::new(),
                },
            )
            .unwrap();
        assert!(matches!(result, CommitResult::WorkingRecorded { .. }));
        let locality = working.counters().unwrap();
        assert_eq!(locality.candidate_full_scans, 0, "{locality:?}");
        assert!(locality.candidate_shallow_bindings >= 3, "{locality:?}");
        let accepted = working.branch_head(head.branch_id).unwrap().unwrap();
        assert_eq!(accepted.root, candidate);

        let dirty_admission = working.begin_operation(accepted).unwrap();
        let dirty_ticket = WorkspaceTicket::from_admission(&dirty_admission, Presentation::Mount);
        let mut custody = WorkspacePaths::create(working.root(), &dirty_ticket).unwrap();
        let dirty_spool = custody.spool().join("dirty-ranges");
        let mut dirty = MountedWorkspace::open(
            &working_root,
            dirty_admission,
            IntegrityMode::TrustedLocalDev,
            dirty_spool.clone(),
        )
        .unwrap();
        let file = dirty.lookup_child(ROOT_NODE, b"file").unwrap();
        let handle = dirty.open_file(file.node, false).unwrap();
        dirty
            .write(file.node, handle, 0, b"uncheckpointed")
            .unwrap();
        let original_spool = std::fs::read(&dirty_spool).unwrap();
        drop(dirty);
        assert!(matches!(
            MountedWorkspace::open(
                &working_root,
                dirty_admission,
                IntegrityMode::TrustedLocalDev,
                dirty_spool.clone(),
            ),
            Err(MountedError::Startup("spool", _))
        ));
        assert_eq!(std::fs::read(&dirty_spool).unwrap(), original_spool);
        assert_eq!(working.branch_head(head.branch_id).unwrap(), Some(accepted));
        working
            .discard_operation(dirty_admission.operation_id)
            .unwrap();
        custody.remove_owned().unwrap();
        drop(working);
        std::fs::remove_dir_all(directory).unwrap();
    }
}
