use super::*;

#[test]
fn raw_candidate_with_missing_child_cannot_use_the_trusted_path() {
    let base = std::env::temp_dir().join(format!(
        "layerfs-forged-candidate-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&base).unwrap();
    let engine =
        Engine::open_with_mode(base.join("store.sqlite"), IntegrityMode::TrustedLocalDev).unwrap();
    let root = valid_empty_root(&engine);
    let stack = engine
        .product_create_layer_stack(
            LayerStackId::from_bytes([0xe1; 32]),
            LayerId::from_bytes([0xe2; 32]),
            "forged-candidate",
            root,
        )
        .unwrap();
    let branch = engine
        .product_create_top_level_branch(
            BranchId::from_bytes([0xe3; 32]),
            Some("forged-candidate"),
            stack,
        )
        .unwrap();
    let operation_id = OperationId::from_bytes([0xe4; 32]);
    engine
        .product_begin_operation(operation_id, branch, LeaseId::from_bytes([0xe5; 32]))
        .unwrap();
    let mut writer = engine.begin_candidate_write().unwrap();
    let forged = writer
        .put(
            &encode_namespace_root(NamespaceRootV1 {
                profile_id: profile_id(),
                root_directory_inode: InodeId::allocate([0xe6; 32], 0),
                inode_table_root: layerfs_core::ObjectId::for_bytes(b"missing inode table"),
            })
            .unwrap(),
        )
        .unwrap();
    assert!(writer
        .commit_operation_candidate(operation_id, forged)
        .is_err());
    assert_eq!(
        engine.product_branch_head(branch.branch_id).unwrap(),
        Some(branch)
    );
    assert!(!engine.contains_authenticated_object(forged).unwrap());
    engine.product_discard_operation(operation_id).unwrap();
    drop(engine);
    fs::remove_dir_all(base).unwrap();
}

#[test]
fn sealed_candidate_cannot_cross_storage_with_only_its_top_root() {
    let base = std::env::temp_dir().join(format!(
        "layerfs-cross-store-candidate-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&base).unwrap();
    let source =
        Engine::open_with_mode(base.join("source.sqlite"), IntegrityMode::TrustedLocalDev).unwrap();
    let destination = Engine::open_with_mode(
        base.join("destination.sqlite"),
        IntegrityMode::TrustedLocalDev,
    )
    .unwrap();
    let root = valid_empty_root(&source);
    let mut after = None;
    loop {
        let ids = source.object_ids_page(after, 64).unwrap();
        if ids.is_empty() {
            break;
        }
        let objects = ids
            .iter()
            .map(|id| {
                (
                    *id,
                    source
                        .load_canonical_authenticated_bounded(*id, 1024 * 1024)
                        .unwrap(),
                )
            })
            .collect::<Vec<_>>();
        destination.accept_canonical_batch(&objects).unwrap();
        after = ids.last().copied();
    }
    let stack_id = LayerStackId::from_bytes([0xf1; 32]);
    let layer_id = LayerId::from_bytes([0xf2; 32]);
    let branch_id = BranchId::from_bytes([0xf3; 32]);
    for engine in [&source, &destination] {
        let stack = engine
            .product_create_layer_stack(stack_id, layer_id, "cross-store", root)
            .unwrap();
        engine
            .product_create_top_level_branch(branch_id, Some("cross-store"), stack)
            .unwrap();
    }
    let mut source_writer = source.begin_candidate_write().unwrap();
    let resolved = logical::resolve(
        &source_writer,
        root,
        &CanonicalPath::root(),
        &mut logical::LogicalCounters::default(),
    )
    .unwrap();
    let inode = source_writer.allocate_inode_id().unwrap();
    let candidate = source_writer
        .trusted_create_directory(
            root,
            &CanonicalPath::new("foreign").unwrap(),
            inode,
            resolved.record.metadata_root,
        )
        .unwrap();
    let candidate_root = candidate.root();
    let top = source_writer.get(candidate_root).unwrap();
    source_writer.commit_objects().unwrap();

    let branch = destination.product_branch_head(branch_id).unwrap().unwrap();
    let operation = OperationId::from_bytes([0xf4; 32]);
    destination
        .product_begin_operation(operation, branch, LeaseId::from_bytes([0xf5; 32]))
        .unwrap();
    let mut destination_writer = destination.begin_candidate_write().unwrap();
    assert_eq!(destination_writer.put(&top).unwrap(), candidate_root);
    assert!(destination_writer
        .commit_trusted_operation_candidate(operation, candidate)
        .is_err());
    assert_eq!(
        destination.product_branch_head(branch_id).unwrap(),
        Some(branch)
    );
    assert!(!destination
        .contains_authenticated_object(candidate_root)
        .unwrap());
    destination.product_discard_operation(operation).unwrap();
    drop(destination);
    drop(source);
    fs::remove_dir_all(base).unwrap();
}

#[test]
fn sealed_candidate_cannot_cross_writers_after_creator_rollback() {
    let base = std::env::temp_dir().join(format!(
        "layerfs-cross-writer-candidate-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&base).unwrap();
    let engine =
        Engine::open_with_mode(base.join("store.sqlite"), IntegrityMode::TrustedLocalDev).unwrap();
    let root = valid_empty_root(&engine);
    let stack = engine
        .product_create_layer_stack(
            LayerStackId::from_bytes([0xf6; 32]),
            LayerId::from_bytes([0xf7; 32]),
            "cross-writer",
            root,
        )
        .unwrap();
    let branch = engine
        .product_create_top_level_branch(
            BranchId::from_bytes([0xf8; 32]),
            Some("cross-writer"),
            stack,
        )
        .unwrap();
    let operation = OperationId::from_bytes([0xf9; 32]);
    engine
        .product_begin_operation(operation, branch, LeaseId::from_bytes([0xfa; 32]))
        .unwrap();
    let mut creator = engine.begin_candidate_write().unwrap();
    let resolved = logical::resolve(
        &creator,
        root,
        &CanonicalPath::root(),
        &mut logical::LogicalCounters::default(),
    )
    .unwrap();
    let inode = creator.allocate_inode_id().unwrap();
    let candidate = creator
        .trusted_create_directory(
            root,
            &CanonicalPath::new("foreign-writer").unwrap(),
            inode,
            resolved.record.metadata_root,
        )
        .unwrap();
    let candidate_root = candidate.root();
    let top = creator.get(candidate_root).unwrap();
    drop(creator);

    let mut second = engine.begin_candidate_write().unwrap();
    assert_eq!(second.put(&top).unwrap(), candidate_root);
    assert!(second
        .commit_trusted_operation_candidate(operation, candidate)
        .is_err());
    assert_eq!(
        engine.product_branch_head(branch.branch_id).unwrap(),
        Some(branch)
    );
    assert!(!engine
        .contains_authenticated_object(candidate_root)
        .unwrap());
    engine.product_discard_operation(operation).unwrap();
    drop(engine);
    fs::remove_dir_all(base).unwrap();
}
