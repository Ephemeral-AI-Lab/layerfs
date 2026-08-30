use layerfs_content::filesystem::ContentChange;
use layerfs_content::object::access::ObjectRead;
use layerfs_content::{CanonicalName, DirectoryEntry, Object, ObjectKind, ObjectReference};
use layerfs_storage::{
    apply_changes, decode_fact, empty_root, encode_fact, transfer_facts, transfer_roots,
    AdmissionSetReceipt, BranchFact, BranchId, BranchRecord, BranchScope, BranchScopeRecord,
    BuiltRoot, CanonicalObject, CommitId, CommitRecord, CoreReader, EntityName, Fact, FactKind,
    LayerId, LayerRecord, LayerStackFact, LayerStackId, LayerStackRecord, LayerStackScopeRecord,
    MissingBitmap, ObjectSource, PushResult, RemotePlacement, RootTransferRequest, StorageError,
    StoreDb, StoreId, StoreRole, TransferTarget,
};

#[test]
fn entity_names_and_immutable_fact_wire_are_exact() {
    for valid in [
        "a",
        "api-server",
        "web.client_2",
        "release.2026",
        &"a".repeat(63),
    ] {
        assert_eq!(EntityName::new(valid).unwrap().as_str(), valid);
    }
    for invalid in [
        "",
        "Main",
        "feature/foo",
        "../escape",
        "name with spaces",
        "-leading",
        "trailing-",
        &"a".repeat(64),
        "é",
    ] {
        assert!(matches!(
            EntityName::new(invalid),
            Err(StorageError::InvalidInput("entity name"))
        ));
    }

    let stack = LayerStackId::new();
    let layer = LayerId::derive(stack, None, layerfs_content::ObjectId::for_bytes(b"root"));
    let origin = BranchId::new();
    let commit = CommitId::derive(layerfs_content::ObjectId::for_bytes(b"commit"), None, layer);
    let facts = [
        Fact::LayerStack(LayerStackFact {
            id: stack,
            name: name("api-server"),
        }),
        Fact::Branch(BranchFact {
            id: BranchId::new(),
            layer_stack_id: stack,
            name: name("main"),
            forked_from_layer_id: None,
            forked_from_branch_id: Some(origin),
            forked_from_commit_id: Some(commit),
        }),
    ];
    for fact in &facts {
        assert_eq!(decode_fact(&encode_fact(fact)).unwrap(), *fact);
    }
    let mut malformed = encode_fact(&facts[0]);
    malformed[19] = b'/';
    assert!(matches!(
        decode_fact(&malformed),
        Err(StorageError::Integrity("wire entity name"))
    ));

    let bitmap = MissingBitmap::from_missing([0, 511]).unwrap();
    assert!(bitmap.is_missing(0).unwrap() && bitmap.is_missing(511).unwrap());
}

#[test]
fn schema_v3_has_exact_census_and_rejects_v2() {
    let root = run_dir("schema");
    let authority_path = root.join("authority.sqlite");
    let branch_path = root.join("branch.sqlite");
    let authority = StoreDb::create(&authority_path, StoreRole::LayerStack, None).unwrap();
    let parent = authority.store_id();
    let branch = StoreDb::create(&branch_path, StoreRole::Branch, Some(parent)).unwrap();
    drop(branch);
    drop(authority);

    let connection = rusqlite::Connection::open(&authority_path).unwrap();
    assert_eq!(pragma(&connection, "user_version"), 3);
    assert_eq!(census(&connection), (6, 25));
    assert_eq!(explicit_index_count(&connection), 11);
    assert_eq!(
        columns(&connection, "branches"),
        [
            "branch_id",
            "layer_stack_id",
            "name",
            "base_layer_id",
            "head_commit_id",
            "forked_from_layer_id",
            "forked_from_branch_id",
            "forked_from_commit_id"
        ]
    );
    connection.pragma_update(None, "user_version", 2).unwrap();
    drop(connection);
    assert!(matches!(
        StoreDb::connect(&authority_path, StoreRole::LayerStack, None),
        Err(StorageError::WrongStoreSchema)
    ));
    let connection = rusqlite::Connection::open(&branch_path).unwrap();
    assert_eq!(census(&connection), (9, 33));
    assert_eq!(explicit_index_count(&connection), 12);
    assert_eq!(
        columns(&connection, "branch_scopes"),
        [
            "branch_id",
            "scope_kind",
            "through_commit_id",
            "serving_mode"
        ]
    );
    assert!(connection
        .execute(
            "INSERT INTO layer_stacks(layer_stack_id,name) VALUES(?1,'Invalid/Name')",
            [LayerStackId::new().to_bytes().as_slice()]
        )
        .is_err());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn exact_fact_membership_and_name_conflicts_compare_full_facts() {
    let root = run_dir("facts");
    let branch = branch_store(&root);
    let stack_a = LayerStackFact {
        id: LayerStackId::new(),
        name: name("api-server"),
    };
    branch
        .admit_facts(&[Fact::LayerStack(stack_a.clone())])
        .unwrap();
    assert!(branch
        .fact_page(FactKind::LayerStack, None, 128)
        .unwrap()
        .0
        .is_empty());
    assert!(!branch
        .missing_facts(&[Fact::LayerStack(stack_a.clone())])
        .unwrap()
        .is_missing(0)
        .unwrap());
    let collision = LayerStackFact {
        id: stack_a.id,
        name: name("different"),
    };
    assert!(matches!(
        branch.missing_facts(&[Fact::LayerStack(collision)]),
        Err(StorageError::Integrity("fact collision"))
    ));
    let stack_b = LayerStackFact {
        id: LayerStackId::new(),
        name: stack_a.name.clone(),
    };
    assert!(matches!(
        branch.admit_facts(&[Fact::LayerStack(stack_b.clone())]),
        Err(StorageError::LayerStackNameConflict {
            existing_id,
            incoming_id,
            ..
        }) if existing_id == stack_a.id && incoming_id == stack_b.id
    ));

    let root_id = layerfs_content::ObjectId::for_bytes(b"layer");
    let layer = layer(stack_a.id, None, root_id);
    branch.insert_layer_fact(layer).unwrap();
    branch
        .publish_layer_stack_scope(LayerStackScopeRecord {
            layer_stack_id: stack_a.id,
            through_layer_id: layer.id,
            serving_mode: RemotePlacement::Reference,
        })
        .unwrap();
    assert_eq!(
        branch.fact_page(FactKind::LayerStack, None, 128).unwrap().0,
        [Fact::LayerStack(stack_a.clone())]
    );
    let first = BranchFact {
        id: BranchId::new(),
        layer_stack_id: stack_a.id,
        name: name("main"),
        forked_from_layer_id: Some(layer.id),
        forked_from_branch_id: None,
        forked_from_commit_id: None,
    };
    branch.insert_branch_fact(&first).unwrap();
    assert!(branch.branch(first.id).unwrap().is_none());
    assert!(branch
        .fact_page(FactKind::Branch, None, 128)
        .unwrap()
        .0
        .is_empty());
    assert_eq!(branch.branch_fact(first.id).unwrap(), Some(first.clone()));
    let second = BranchFact {
        id: BranchId::new(),
        ..first.clone()
    };
    assert!(matches!(
        branch.insert_branch_fact(&second),
        Err(StorageError::BranchNameConflict {
            existing_id,
            incoming_id,
            ..
        }) if existing_id == first.id && incoming_id == second.id
    ));
    drop(branch);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn deterministic_layer_and_commit_ids_are_recomputed_before_admission() {
    let root = run_dir("deterministic-ids");
    let store = branch_store(&root);
    let stack = LayerStackFact {
        id: LayerStackId::new(),
        name: name("project"),
    };
    store
        .admit_facts(&[Fact::LayerStack(stack.clone())])
        .unwrap();
    let actual_root = layerfs_content::ObjectId::for_bytes(b"actual");
    let wrong_root = layerfs_content::ObjectId::for_bytes(b"wrong");
    let invalid_layer = LayerRecord {
        id: LayerId::derive(stack.id, None, wrong_root),
        layer_stack_id: stack.id,
        parent_layer_id: None,
        root_id: actual_root,
        source_branch_id: None,
        source_commit_id: None,
    };
    assert_eq!(
        store.insert_layer_fact(invalid_layer),
        Err(StorageError::Integrity("Layer identity"))
    );
    let valid_layer = layer(stack.id, None, actual_root);
    store.insert_layer_fact(valid_layer).unwrap();
    let invalid_commit = CommitRecord {
        id: CommitId::derive(wrong_root, None, valid_layer.id),
        root_id: actual_root,
        parent_commit_id: None,
        base_layer_id: valid_layer.id,
    };
    assert_eq!(
        store.insert_commit_fact(invalid_commit),
        Err(StorageError::Integrity("Commit identity"))
    );
    drop(store);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn scope_publication_is_visibility_last_and_remote_advance_is_atomic() {
    let root = run_dir("scope");
    let store = branch_store(&root);
    let stack = LayerStackFact {
        id: LayerStackId::new(),
        name: name("project"),
    };
    store
        .admit_facts(&[Fact::LayerStack(stack.clone())])
        .unwrap();
    let root_id = layerfs_content::ObjectId::for_bytes(b"root");
    let base = layer(stack.id, None, root_id);
    store.insert_layer_fact(base).unwrap();
    assert!(store.layer_stack(stack.id).unwrap().is_none());
    store
        .publish_layer_stack_scope(LayerStackScopeRecord {
            layer_stack_id: stack.id,
            through_layer_id: base.id,
            serving_mode: RemotePlacement::Reference,
        })
        .unwrap();
    assert_eq!(
        store.layer_stack(stack.id).unwrap().unwrap().head_layer_id,
        base.id
    );
    let child = LayerRecord {
        id: LayerId::derive(stack.id, Some(base.id), root_id),
        layer_stack_id: stack.id,
        parent_layer_id: Some(base.id),
        root_id,
        source_branch_id: Some(BranchId::new()),
        source_commit_id: Some(CommitId::derive(root_id, None, base.id)),
    };
    store.insert_layer_fact(child).unwrap();
    store
        .publish_layer_stack_scope(LayerStackScopeRecord {
            layer_stack_id: stack.id,
            through_layer_id: child.id,
            serving_mode: RemotePlacement::Replica,
        })
        .unwrap();
    assert_eq!(
        store.publish_layer_stack_scope(LayerStackScopeRecord {
            layer_stack_id: stack.id,
            through_layer_id: base.id,
            serving_mode: RemotePlacement::Reference,
        }),
        Err(StorageError::LayerHeadMoved {
            expected: child.id,
            actual: base.id,
        })
    );
    store
        .publish_layer_stack_scope(LayerStackScopeRecord {
            layer_stack_id: stack.id,
            through_layer_id: child.id,
            serving_mode: RemotePlacement::Reference,
        })
        .unwrap();

    let commits = commit_chain(base.id, root_id, 6);
    for commit in &commits {
        store.insert_commit_fact(*commit).unwrap();
    }
    let fact = BranchFact {
        id: BranchId::new(),
        layer_stack_id: stack.id,
        name: name("remote"),
        forked_from_layer_id: Some(base.id),
        forked_from_branch_id: None,
        forked_from_commit_id: None,
    };
    store.insert_branch_fact(&fact).unwrap();
    let at_c3 = BranchRecord {
        id: fact.id,
        layer_stack_id: fact.layer_stack_id,
        name: fact.name.clone(),
        base_layer_id: base.id,
        head_commit_id: Some(commits[2].id),
        forked_from_layer_id: fact.forked_from_layer_id,
        forked_from_branch_id: None,
        forked_from_commit_id: None,
    };
    store
        .publish_remote_branch_scope(
            &at_c3,
            BranchScopeRecord {
                branch_id: fact.id,
                scope: BranchScope::Remote {
                    through_commit_id: commits[2].id,
                    serving_mode: RemotePlacement::Reference,
                },
            },
        )
        .unwrap();
    let at_c6 = BranchRecord {
        head_commit_id: Some(commits[5].id),
        ..at_c3
    };
    store
        .publish_remote_branch_scope(
            &at_c6,
            BranchScopeRecord {
                branch_id: fact.id,
                scope: BranchScope::Remote {
                    through_commit_id: commits[5].id,
                    serving_mode: RemotePlacement::Replica,
                },
            },
        )
        .unwrap();
    let pinned = store.pin_branch(fact.id).unwrap().unwrap();
    assert_eq!(pinned.branch.head_commit_id, Some(commits[5].id));
    assert_eq!(
        pinned.scope.scope,
        BranchScope::Remote {
            through_commit_id: commits[5].id,
            serving_mode: RemotePlacement::Replica
        }
    );
    assert_eq!(pinned.root_id, root_id);
    let backward = BranchRecord {
        head_commit_id: Some(commits[2].id),
        ..pinned.branch.clone()
    };
    assert_eq!(
        store.publish_remote_branch_scope(
            &backward,
            BranchScopeRecord {
                branch_id: fact.id,
                scope: BranchScope::Remote {
                    through_commit_id: commits[2].id,
                    serving_mode: RemotePlacement::Reference,
                },
            },
        ),
        Err(StorageError::CommitHeadMoved {
            expected: Some(commits[5].id),
            actual: Some(commits[2].id),
        })
    );
    store
        .publish_remote_branch_scope(
            &at_c6,
            BranchScopeRecord {
                branch_id: fact.id,
                scope: BranchScope::Remote {
                    through_commit_id: commits[5].id,
                    serving_mode: RemotePlacement::Reference,
                },
            },
        )
        .unwrap();
    assert_eq!(
        store.branch_scope(fact.id).unwrap().unwrap().scope,
        BranchScope::Remote {
            through_commit_id: commits[5].id,
            serving_mode: RemotePlacement::Reference
        }
    );

    let local_fact = branch_fact(stack.id, base.id, "local");
    store.insert_branch_fact(&local_fact).unwrap();
    store
        .publish_local_branch(&branch_record(&local_fact, base.id, None))
        .unwrap();
    let stack_page = store.layer_stack_record_page(None, 1).unwrap();
    assert_eq!(stack_page.records.len(), 1);
    assert_eq!(stack_page.continuation, None);
    assert_eq!(
        store.layer_stack_scope_page(None, 1).unwrap().records.len(),
        1
    );
    let first = store.branch_record_page(Some(stack.id), None, 1).unwrap();
    assert_eq!(first.records.len(), 1);
    let continuation = first.continuation.expect("second visible Branch");
    let second = store
        .branch_record_page(Some(stack.id), Some(continuation), 1)
        .unwrap();
    assert_eq!(second.records.len(), 1);
    assert_eq!(second.continuation, None);
    assert_eq!(
        store
            .branch_scope_page(Some(stack.id), None, 512)
            .unwrap()
            .records
            .len(),
        2
    );
    assert_eq!(
        store
            .branch_record_page(None, None, 512)
            .unwrap()
            .records
            .len(),
        2
    );
    assert_eq!(
        store
            .branch_scope_page(None, None, 512)
            .unwrap()
            .records
            .len(),
        2
    );
    assert!(matches!(
        store.layer_stack_record_page(None, 0),
        Err(StorageError::InvalidInput("record page"))
    ));
    drop(store);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn remote_branches_are_read_only_and_local_branches_can_commit() {
    let root = run_dir("ownership");
    let store = branch_store(&root);
    let (stack, base) = receiver_stack(&store, "project");
    let fact = branch_fact(stack.id, base.id, "work");
    store.insert_branch_fact(&fact).unwrap();
    let record = branch_record(&fact, base.id, None);
    store
        .publish_remote_branch_scope(
            &record,
            BranchScopeRecord {
                branch_id: fact.id,
                scope: BranchScope::Remote {
                    through_commit_id: CommitId::derive(base.root_id, None, base.id),
                    serving_mode: RemotePlacement::Reference,
                },
            },
        )
        .unwrap_err();

    let initial = CommitRecord {
        id: CommitId::derive(base.root_id, None, base.id),
        root_id: base.root_id,
        parent_commit_id: None,
        base_layer_id: base.id,
    };
    store.insert_commit_fact(initial).unwrap();
    let remote = BranchRecord {
        head_commit_id: Some(initial.id),
        ..record.clone()
    };
    store
        .publish_remote_branch_scope(
            &remote,
            BranchScopeRecord {
                branch_id: fact.id,
                scope: BranchScope::Remote {
                    through_commit_id: initial.id,
                    serving_mode: RemotePlacement::Reference,
                },
            },
        )
        .unwrap();
    let next = CommitRecord {
        id: CommitId::derive(base.root_id, Some(initial.id), base.id),
        root_id: base.root_id,
        parent_commit_id: Some(initial.id),
        base_layer_id: base.id,
    };
    assert_eq!(
        store.commit_branch(fact.id, Some(initial.id), base.id, next, base.id, false),
        Err(StorageError::ReadOnlyBranch(fact.id))
    );

    let local_fact = branch_fact(stack.id, base.id, "local");
    store.insert_branch_fact(&local_fact).unwrap();
    let local = branch_record(&local_fact, base.id, None);
    store.publish_local_branch(&local).unwrap();
    let local_commit = CommitRecord {
        id: CommitId::derive(base.root_id, None, base.id),
        root_id: base.root_id,
        parent_commit_id: None,
        base_layer_id: base.id,
    };
    store
        .commit_branch(local.id, None, base.id, local_commit, base.id, false)
        .unwrap();
    assert_eq!(
        store.publish_local_branch(&local),
        Err(StorageError::Integrity("local Branch publication"))
    );
    drop(store);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn history_and_layer_prefix_pages_are_bounded_and_cursor_exact() {
    let root = run_dir("pages");
    let authority =
        StoreDb::create(root.join("authority.sqlite"), StoreRole::LayerStack, None).unwrap();
    let built = empty_root([31; 32]).unwrap();
    admit_built(&authority, &built);
    let stack = LayerStackRecord {
        id: LayerStackId::new(),
        name: name("project"),
        head_layer_id: LayerId::derive(LayerStackId::new(), None, built.root_id),
    };
    let stack = LayerStackRecord {
        head_layer_id: LayerId::derive(stack.id, None, built.root_id),
        ..stack
    };
    let base = layer(stack.id, None, built.root_id);
    authority.insert_layerstack_genesis(&stack, &base).unwrap();

    let commits = commit_chain(base.id, built.root_id, 270);
    for commit in &commits {
        authority.insert_commit_fact(*commit).unwrap();
    }
    let fact = branch_fact(stack.id, base.id, "main");
    let record = BranchRecord {
        head_commit_id: Some(commits.last().unwrap().id),
        ..branch_record(&fact, base.id, None)
    };
    authority.authority_publish_branch(&record, None).unwrap();
    let mut cursor = None;
    let mut seen = Vec::new();
    loop {
        let page = authority
            .commit_history_page(fact.id, commits.last().unwrap().id, cursor, 128)
            .unwrap();
        assert!(page.records.len() <= 128);
        seen.extend(page.records.iter().map(|record| record.id));
        let Some(next) = page.continuation else { break };
        cursor = Some(next);
    }
    assert_eq!(seen.len(), 270);
    assert_eq!(seen[0], commits.last().unwrap().id);
    assert_eq!(seen[269], commits[0].id);
    assert!(matches!(
        authority.commit_history_page(fact.id, commits[0].id, None, 129),
        Err(StorageError::InvalidInput("history page"))
    ));
    let layers = authority.layer_prefix_page(base.id, None, 128).unwrap();
    assert_eq!(layers.records, vec![base]);
    assert_eq!(layers.continuation, None);

    let mut after = None;
    let mut queried = 0;
    loop {
        #[cfg(feature = "test-instrumentation")]
        layerfs_storage::reset_sql_trace();
        let (page, continuation) = authority
            .fact_page(FactKind::Commit, after.as_deref(), 73)
            .unwrap();
        assert!(page.len() <= 73);
        #[cfg(feature = "test-instrumentation")]
        assert_eq!(
            layerfs_storage::sql_trace()
                .iter()
                .filter(|sql| sql.contains("FROM commits WHERE commit_id"))
                .count(),
            1,
            "fact page must be one direct query"
        );
        queried += page.len();
        let Some(next) = continuation else { break };
        assert_eq!(page.last().unwrap().id(), next);
        after = Some(next);
    }
    assert_eq!(queried, commits.len());
    assert!(matches!(
        authority.fact_page(FactKind::Commit, Some(&[0]), 1),
        Err(StorageError::InvalidInput("fact query cursor"))
    ));
    let explain = rusqlite::Connection::open(authority.path())
        .unwrap()
        .prepare(
            "EXPLAIN QUERY PLAN
             SELECT commit_id,root_id,parent_commit_id,base_layer_id FROM commits
             WHERE commit_id>?1 ORDER BY commit_id LIMIT ?2",
        )
        .unwrap()
        .query_map(rusqlite::params![vec![0_u8; 33], 74_i64], |row| {
            row.get::<_, String>(3)
        })
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap();
    assert!(explain.iter().any(|detail| detail.contains("SEARCH")));
    assert!(explain.iter().all(|detail| !detail.starts_with("SCAN ")));
    drop(authority);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn multi_root_transfer_and_receipts_share_one_operation_state() {
    let root = run_dir("root-union");
    let authority =
        StoreDb::create(root.join("authority.sqlite"), StoreRole::LayerStack, None).unwrap();
    let receiver = StoreDb::create(
        root.join("branch.sqlite"),
        StoreRole::Branch,
        Some(authority.store_id()),
    )
    .unwrap();
    let built = empty_root([41; 32]).unwrap();
    admit_built(&authority, &built);
    let order = layerfs_storage::dependency_order(&DbSource(&authority), built.root_id).unwrap();
    let reused_id = order[0];
    let reused_bytes = authority.read_object_row(reused_id).unwrap();
    receiver
        .admit_objects(&[CanonicalObject {
            id: reused_id,
            bytes: reused_bytes.clone(),
        }])
        .unwrap();
    let receipt = transfer_roots(
        &DbSource(&authority),
        &receiver,
        [
            RootTransferRequest {
                root_id: built.root_id,
                known_complete: false,
            },
            RootTransferRequest {
                root_id: built.root_id,
                known_complete: false,
            },
        ],
    )
    .unwrap();
    assert!(receipt.objects.sent_ids > 0);
    assert_eq!(
        receipt.objects.announced_bytes.exact().unwrap(),
        order
            .iter()
            .map(|id| authority.read_object_row(*id).unwrap().len() as u64)
            .sum::<u64>()
    );
    assert_eq!(receipt.objects.preexisting_ids(), 1);
    assert_eq!(
        receipt.objects.preexisting_bytes().exact(),
        Some(reused_bytes.len() as u64)
    );
    assert_eq!(
        receipt.objects.announced_ids,
        receipt.objects.sent_ids + receipt.objects.preexisting_ids()
    );
    assert_eq!(
        receiver
            .verify_and_record_complete_roots([built.root_id, built.root_id])
            .unwrap(),
        1
    );
    assert!(receiver.complete_root(built.root_id).unwrap());
    drop(receiver);
    drop(authority);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn large_object_transfer_is_streamed_and_membership_reads_only_lengths() {
    let root = run_dir("large-object-transfer");
    let authority =
        StoreDb::create(root.join("authority.sqlite"), StoreRole::LayerStack, None).unwrap();
    let receiver = StoreDb::create(
        root.join("branch.sqlite"),
        StoreRole::Branch,
        Some(authority.store_id()),
    )
    .unwrap();
    let canonical = layerfs_content::encode_bytes_object(&vec![7_u8; 5 * 1024 * 1024]).unwrap();
    let object = CanonicalObject::new(canonical).unwrap();
    authority
        .admit_objects(std::slice::from_ref(&object))
        .unwrap();

    let receipt = transfer_roots(
        &DbSource(&authority),
        &receiver,
        [RootTransferRequest {
            root_id: object.id,
            known_complete: false,
        }],
    )
    .unwrap();
    assert_eq!(receipt.objects.sent_ids, 1);
    assert_eq!(
        receipt.objects.announced_bytes.exact(),
        Some(object.bytes.len() as u64)
    );
    assert!(receipt.peak_buffer_bytes >= object.bytes.len() as u64);
    assert!(receipt.peak_buffer_bytes < layerfs_storage::TRANSFER_BUFFER_BYTES as u64);

    #[cfg(feature = "test-instrumentation")]
    {
        layerfs_storage::reset_sql_trace();
        assert!(!receiver
            .missing_objects(&[object.id])
            .unwrap()
            .is_missing(0)
            .unwrap());
        let trace = layerfs_storage::sql_trace();
        assert!(trace.iter().any(|sql| sql.contains("length(bytes)")));
        assert!(trace
            .iter()
            .all(|sql| !sql.contains("SELECT object_id,bytes")));
    }

    drop(receiver);
    drop(authority);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn transfer_is_postorder_and_rejects_a_nested_payload_over_the_buffer_ceiling() {
    let (small_source, small_order) = nested_objects(3, 1);
    let target = ObjectRecordingTarget::default();
    transfer_roots(
        &small_source,
        &target,
        [RootTransferRequest {
            root_id: *small_order.last().unwrap(),
            known_complete: false,
        }],
    )
    .unwrap();
    assert_eq!(*target.admitted.lock().unwrap(), small_order);

    let (large_source, large_order) = nested_objects(3, 60_000);
    assert!(
        large_order
            .iter()
            .skip(1)
            .map(|id| large_source.0[id].len())
            .sum::<usize>()
            >= layerfs_storage::TRANSFER_BUFFER_BYTES
    );
    assert!(matches!(
        transfer_roots(
            &large_source,
            &ObjectRecordingTarget::default(),
            [RootTransferRequest {
                root_id: *large_order.last().unwrap(),
                known_complete: false,
            }],
        ),
        Err(StorageError::Integrity("transfer buffer ceiling"))
    ));
}

#[test]
fn fact_transfer_batches_membership_but_preserves_dependency_order() {
    let stack = LayerStackId::new();
    let base = LayerId::derive(stack, None, layerfs_content::ObjectId::for_bytes(b"base"));
    let commits = commit_chain(base, layerfs_content::ObjectId::for_bytes(b"history"), 300);
    let facts = commits
        .iter()
        .copied()
        .map(Fact::Commit)
        .collect::<Vec<_>>();
    let target = RecordingTarget::default();
    let receipt = transfer_facts(&target, &facts).unwrap();
    assert_eq!(receipt.facts[&FactKind::Commit].sent_ids, 300);
    assert_eq!(
        *target.admitted.lock().unwrap(),
        commits.iter().map(|commit| commit.id).collect::<Vec<_>>()
    );
    assert!(target
        .admission_batch_sizes
        .lock()
        .unwrap()
        .iter()
        .all(|size| *size <= 128));
}

#[test]
fn authority_cas_rejects_an_observed_head_outside_the_incoming_lane() {
    let root = run_dir("authority-cas");
    let authority =
        StoreDb::create(root.join("authority.sqlite"), StoreRole::LayerStack, None).unwrap();
    let built = empty_root([51; 32]).unwrap();
    admit_built(&authority, &built);
    let stack = LayerStackRecord {
        id: LayerStackId::new(),
        name: name("project"),
        head_layer_id: LayerId::derive(LayerStackId::new(), None, built.root_id),
    };
    let stack = LayerStackRecord {
        head_layer_id: LayerId::derive(stack.id, None, built.root_id),
        ..stack
    };
    let base = layer(stack.id, None, built.root_id);
    authority.insert_layerstack_genesis(&stack, &base).unwrap();
    let first = CommitRecord {
        id: CommitId::derive(built.root_id, None, base.id),
        root_id: built.root_id,
        parent_commit_id: None,
        base_layer_id: base.id,
    };
    authority.insert_commit_fact(first).unwrap();
    let other_built = empty_root([52; 32]).unwrap();
    admit_built(&authority, &other_built);
    let unrelated = CommitRecord {
        id: CommitId::derive(other_built.root_id, None, base.id),
        root_id: other_built.root_id,
        parent_commit_id: None,
        base_layer_id: base.id,
    };
    authority.insert_commit_fact(unrelated).unwrap();
    let fact = branch_fact(stack.id, base.id, "main");
    let initial = BranchRecord {
        head_commit_id: Some(first.id),
        ..branch_record(&fact, base.id, None)
    };
    assert!(matches!(
        authority.authority_publish_branch(&initial, None).unwrap(),
        PushResult::Created { .. }
    ));
    let invalid = BranchRecord {
        head_commit_id: Some(unrelated.id),
        ..initial.clone()
    };
    assert_eq!(
        authority
            .authority_publish_branch(&invalid, Some(first.id))
            .unwrap(),
        PushResult::HeadMoved {
            authority_head: first.id,
            local_head: unrelated.id
        }
    );
    assert_eq!(
        authority.branch(fact.id).unwrap().unwrap().head_commit_id,
        Some(first.id)
    );
    drop(authority);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn core_reader_preserves_storage_failure_classes() {
    let id = layerfs_content::ObjectId::for_bytes(b"missing");
    assert_eq!(
        CoreReader(&FailingSource(Failure::Missing)).get(id),
        Err(layerfs_content::CoreError::MissingObject)
    );
    assert_eq!(
        CoreReader(&FailingSource(Failure::Integrity)).get(id),
        Err(layerfs_content::CoreError::InvalidRecord("damaged closure"))
    );
    assert_eq!(
        CoreReader(&FailingSource(Failure::Unavailable)).get(id),
        Err(layerfs_content::CoreError::ValidationAuthorityUnavailable)
    );
    let changes = [ContentChange::Write {
        path: "file".to_owned(),
        bytes: b"value".to_vec(),
        mode: 0o644,
    }];
    assert!(matches!(
        apply_changes(&FailingSource(Failure::Integrity), id, &changes, [1; 32]),
        Err(StorageError::Integrity("damaged closure"))
    ));
    assert!(matches!(
        apply_changes(&FailingSource(Failure::Unavailable), id, &changes, [2; 32]),
        Err(StorageError::Unavailable)
    ));
}

#[cfg(feature = "test-instrumentation")]
#[test]
fn expensive_validation_precedes_every_publication_write() {
    use layerfs_storage::{reset_sql_trace, sql_trace};

    let root = run_dir("transaction-order");
    let store = branch_store(&root);
    let stack = LayerStackFact {
        id: LayerStackId::new(),
        name: name("project"),
    };
    store
        .admit_facts(&[Fact::LayerStack(stack.clone())])
        .unwrap();
    let actual = layerfs_content::ObjectId::for_bytes(b"actual");
    let invalid = LayerRecord {
        id: LayerId::derive(
            stack.id,
            None,
            layerfs_content::ObjectId::for_bytes(b"wrong"),
        ),
        layer_stack_id: stack.id,
        parent_layer_id: None,
        root_id: actual,
        source_branch_id: None,
        source_commit_id: None,
    };
    reset_sql_trace();
    assert_eq!(
        store.insert_layer_fact(invalid),
        Err(StorageError::Integrity("Layer identity"))
    );
    let invalid_id = invalid.id.to_string();
    assert!(
        sql_trace()
            .iter()
            .all(|sql| !sql.to_ascii_lowercase().contains(&invalid_id)),
        "invalid identity reached SQLite"
    );

    let built = empty_root([61; 32]).unwrap();
    admit_built(&store, &built);
    reset_sql_trace();
    store
        .verify_and_record_complete_roots([built.root_id])
        .unwrap();
    let trace = sql_trace();
    let read = trace
        .iter()
        .position(|sql| sql.contains("SELECT bytes FROM objects"))
        .unwrap();
    let publish = trace
        .iter()
        .rposition(|sql| sql.contains("INSERT INTO complete_roots"))
        .unwrap();
    assert!(
        read < publish,
        "receipt published before closure validation"
    );
    drop(store);
    std::fs::remove_dir_all(root).unwrap();
}

#[derive(Clone, Copy)]
enum Failure {
    Missing,
    Integrity,
    Unavailable,
}

struct FailingSource(Failure);

impl ObjectSource for FailingSource {
    fn read_object(&self, id: layerfs_content::ObjectId) -> layerfs_storage::Result<Vec<u8>> {
        Err(match self.0 {
            Failure::Missing => StorageError::MissingObject(id),
            Failure::Integrity => StorageError::Integrity("damaged closure"),
            Failure::Unavailable => StorageError::Unavailable,
        })
    }
}

struct DbSource<'a>(&'a StoreDb);

impl ObjectSource for DbSource<'_> {
    fn read_object(&self, id: layerfs_content::ObjectId) -> layerfs_storage::Result<Vec<u8>> {
        self.0.read_object_row(id)
    }
}

struct MapSource(std::collections::BTreeMap<layerfs_content::ObjectId, Vec<u8>>);

impl ObjectSource for MapSource {
    fn read_object(&self, id: layerfs_content::ObjectId) -> layerfs_storage::Result<Vec<u8>> {
        self.0
            .get(&id)
            .cloned()
            .ok_or(StorageError::MissingObject(id))
    }
}

fn nested_objects(depth: usize, width: usize) -> (MapSource, Vec<layerfs_content::ObjectId>) {
    let leaf = layerfs_content::encode_bytes_object(b"leaf").unwrap();
    let mut child = layerfs_content::ObjectId::for_bytes(&leaf);
    let mut kind = ObjectKind::Bytes;
    let mut rows = std::collections::BTreeMap::from([(child, leaf)]);
    let mut order = vec![child];
    let suffix = "x".repeat(190);
    for _ in 0..depth {
        let entries = (0..width)
            .map(|index| {
                DirectoryEntry::new(
                    CanonicalName::new(&format!("{index:05}-{suffix}")).unwrap(),
                    ObjectReference::new(kind, child),
                )
            })
            .collect();
        let canonical =
            layerfs_content::encode_object(&Object::directory(entries).unwrap()).unwrap();
        child = layerfs_content::ObjectId::for_bytes(&canonical);
        kind = ObjectKind::Directory;
        rows.insert(child, canonical);
        order.push(child);
    }
    (MapSource(rows), order)
}

#[derive(Default)]
struct ObjectRecordingTarget {
    admitted: std::sync::Mutex<Vec<layerfs_content::ObjectId>>,
}

impl TransferTarget for ObjectRecordingTarget {
    fn object_membership(
        &self,
        ids: &[layerfs_content::ObjectId],
    ) -> layerfs_storage::Result<(MissingBitmap, Vec<Option<u64>>)> {
        Ok((
            MissingBitmap::from_missing(0..ids.len())?,
            vec![None; ids.len()],
        ))
    }

    fn missing_facts(&self, _facts: &[Fact]) -> layerfs_storage::Result<MissingBitmap> {
        Ok(MissingBitmap::empty())
    }

    fn admit_objects(
        &self,
        objects: &[CanonicalObject],
    ) -> layerfs_storage::Result<AdmissionSetReceipt> {
        self.admitted
            .lock()
            .unwrap()
            .extend(objects.iter().map(|object| object.id));
        Ok(AdmissionSetReceipt {
            inserted_ids: objects.len() as u64,
            inserted_bytes: objects.iter().map(|object| object.bytes.len() as u64).sum(),
            ..AdmissionSetReceipt::default()
        })
    }

    fn admit_facts(&self, _facts: &[Fact]) -> layerfs_storage::Result<AdmissionSetReceipt> {
        unreachable!()
    }
}

#[derive(Default)]
struct RecordingTarget {
    admitted: std::sync::Mutex<Vec<CommitId>>,
    admission_batch_sizes: std::sync::Mutex<Vec<usize>>,
}

impl TransferTarget for RecordingTarget {
    fn object_membership(
        &self,
        ids: &[layerfs_content::ObjectId],
    ) -> layerfs_storage::Result<(MissingBitmap, Vec<Option<u64>>)> {
        Ok((MissingBitmap::empty(), vec![Some(0); ids.len()]))
    }

    fn missing_facts(&self, facts: &[Fact]) -> layerfs_storage::Result<MissingBitmap> {
        assert!(facts.windows(2).all(|pair| pair[0].id() < pair[1].id()));
        MissingBitmap::from_missing(0..facts.len())
    }

    fn admit_objects(
        &self,
        _objects: &[CanonicalObject],
    ) -> layerfs_storage::Result<AdmissionSetReceipt> {
        unreachable!()
    }

    fn admit_facts(&self, facts: &[Fact]) -> layerfs_storage::Result<AdmissionSetReceipt> {
        self.admission_batch_sizes.lock().unwrap().push(facts.len());
        let mut admitted = self.admitted.lock().unwrap();
        for fact in facts {
            let Fact::Commit(commit) = fact else {
                unreachable!()
            };
            admitted.push(commit.id);
        }
        Ok(AdmissionSetReceipt {
            inserted_ids: facts.len() as u64,
            inserted_bytes: facts.iter().map(|fact| fact.encoded_size() as u64).sum(),
            raced_existing_ids: 0,
            raced_existing_bytes: 0,
        })
    }
}

fn receiver_stack(store: &StoreDb, value: &str) -> (LayerStackFact, LayerRecord) {
    let stack = LayerStackFact {
        id: LayerStackId::new(),
        name: name(value),
    };
    store
        .admit_facts(&[Fact::LayerStack(stack.clone())])
        .unwrap();
    let root = layerfs_content::ObjectId::for_bytes(value.as_bytes());
    let layer = layer(stack.id, None, root);
    store.insert_layer_fact(layer).unwrap();
    (stack, layer)
}

fn branch_fact(stack: LayerStackId, base: LayerId, value: &str) -> BranchFact {
    BranchFact {
        id: BranchId::new(),
        layer_stack_id: stack,
        name: name(value),
        forked_from_layer_id: Some(base),
        forked_from_branch_id: None,
        forked_from_commit_id: None,
    }
}

fn branch_record(
    fact: &BranchFact,
    base_layer_id: LayerId,
    head_commit_id: Option<CommitId>,
) -> BranchRecord {
    BranchRecord {
        id: fact.id,
        layer_stack_id: fact.layer_stack_id,
        name: fact.name.clone(),
        base_layer_id,
        head_commit_id,
        forked_from_layer_id: fact.forked_from_layer_id,
        forked_from_branch_id: fact.forked_from_branch_id,
        forked_from_commit_id: fact.forked_from_commit_id,
    }
}

fn layer(
    stack: LayerStackId,
    parent_layer_id: Option<LayerId>,
    root_id: layerfs_content::ObjectId,
) -> LayerRecord {
    LayerRecord {
        id: LayerId::derive(stack, parent_layer_id, root_id),
        layer_stack_id: stack,
        parent_layer_id,
        root_id,
        source_branch_id: None,
        source_commit_id: None,
    }
}

fn commit_chain(base: LayerId, root: layerfs_content::ObjectId, count: usize) -> Vec<CommitRecord> {
    let mut parent = None;
    (0..count)
        .map(|_| {
            let record = CommitRecord {
                id: CommitId::derive(root, parent, base),
                root_id: root,
                parent_commit_id: parent,
                base_layer_id: base,
            };
            parent = Some(record.id);
            record
        })
        .collect()
}

fn name(value: &str) -> EntityName {
    EntityName::new(value).unwrap()
}

fn admit_built(store: &StoreDb, built: &BuiltRoot) {
    built
        .objects
        .visit_batches(&mut |objects, _| {
            store.admit_objects(objects)?;
            Ok(())
        })
        .unwrap();
}

fn branch_store(root: &std::path::Path) -> StoreDb {
    StoreDb::create(
        root.join("branch.sqlite"),
        StoreRole::Branch,
        Some(StoreId::random().unwrap()),
    )
    .unwrap()
}

fn pragma(connection: &rusqlite::Connection, name: &str) -> i64 {
    connection
        .query_row(&format!("PRAGMA {name}"), [], |row| row.get(0))
        .unwrap()
}

fn census(connection: &rusqlite::Connection) -> (i64, i64) {
    connection
        .query_row(
            "SELECT count(*),sum((SELECT count(*) FROM pragma_table_info(s.name)))
             FROM sqlite_schema s WHERE type='table' AND name NOT LIKE 'sqlite_%'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap()
}

fn explicit_index_count(connection: &rusqlite::Connection) -> i64 {
    connection
        .query_row(
            "SELECT count(*) FROM sqlite_schema WHERE type='index' AND sql IS NOT NULL",
            [],
            |row| row.get(0),
        )
        .unwrap()
}

fn columns(connection: &rusqlite::Connection, table: &str) -> Vec<String> {
    connection
        .prepare("SELECT name FROM pragma_table_info(?1) ORDER BY cid")
        .unwrap()
        .query_map([table], |row| row.get(0))
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap()
}

fn run_dir(name: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!(
        "layerfs-v2-storage-{name}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&path).unwrap();
    path
}
