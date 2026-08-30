use layerfs_branch_store::{BranchStore, CommitOutcome};
use layerfs_content::filesystem::ContentChange;
use layerfs_layerstack_store::LayerStackStore;
use layerfs_storage::{
    receipt_totals, take_transfer_receipts, AdmissionSetReceipt, AuthorityAddResult, BranchFact,
    BranchId, BranchRecord, CanonicalObject, CommitHistoryPage, CommitId, CommitRecord, EntityName,
    Fact, FactKind, LayerId, LayerPrefixPage, LayerRecord, LayerStackEndpoint, LayerStackFact,
    LayerStackId, LayerStackInitialization, LayerStackRecord, LocalForkSource, MissingBitmap,
    ObjectSource, PullBranchResult, PullLayerResult, PushResult, ReconcileChoice, RemotePlacement,
    StorageError, StoreId,
};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

#[test]
fn layer_pull_tracks_exact_boundary_mode_and_complete_prefix() {
    let root = temp("layer-prefix");
    std::fs::create_dir_all(&root).unwrap();
    let authority = Arc::new(LayerStackStore::create(root.join("authority.sqlite")).unwrap());
    let initialized = authority
        .initialize_layerstack(
            EntityName::new("project").unwrap(),
            LayerStackInitialization::Empty,
        )
        .unwrap();
    let branches = BranchStore::create(root.join("branch.sqlite"), authority.store_id()).unwrap();
    let genesis = initialized.genesis_layer_id;

    assert!(matches!(
        branches
            .pull_layer(authority.clone(), genesis, RemotePlacement::Reference)
            .unwrap(),
        PullLayerResult::Created { .. }
    ));
    assert!(matches!(
        branches
            .pull_layer(authority.clone(), genesis, RemotePlacement::Reference)
            .unwrap(),
        PullLayerResult::UpToDate { .. }
    ));
    let older_endpoint = Arc::new(CountingEndpoint::new(authority.clone()));
    assert!(matches!(
        branches
            .pull_layer(older_endpoint.clone(), genesis, RemotePlacement::Replica)
            .unwrap(),
        PullLayerResult::ModeChanged {
            previous: RemotePlacement::Reference,
            ..
        }
    ));
    let genesis_root = authority.layer(genesis).unwrap().unwrap().root_id;
    assert!(branches.root_complete(genesis_root).unwrap());
    assert!(matches!(
        branches
            .pull_layer(authority.clone(), genesis, RemotePlacement::Reference)
            .unwrap(),
        PullLayerResult::ModeChanged {
            previous: RemotePlacement::Replica,
            ..
        }
    ));
    assert!(branches.root_complete(genesis_root).unwrap());

    let local = branches
        .fork_branch(
            EntityName::new("main").unwrap(),
            LocalForkSource::Layer { layer_id: genesis },
        )
        .unwrap();
    branches
        .commit_changes(
            authority.clone(),
            local,
            None,
            &[ContentChange::Write {
                path: "next".into(),
                bytes: b"next".to_vec(),
                mode: 0o644,
            }],
        )
        .unwrap();
    branches.push_branch(authority.clone(), local).unwrap();
    let next = match authority.add_layer(local).unwrap() {
        layerfs_storage::AuthorityAddResult::Added { layer_id } => layer_id,
        result => panic!("unexpected Add result: {result:?}"),
    };
    assert!(matches!(
        branches
            .pull_layer(authority.clone(), next, RemotePlacement::Reference)
            .unwrap(),
        PullLayerResult::Advanced {
            previous_layer_id,
            through_layer_id,
            ..
        } if previous_layer_id == genesis && through_layer_id == next
    ));
    older_endpoint
        .layer_ancestry_records
        .store(0, Ordering::Relaxed);
    assert!(matches!(
        branches
            .pull_layer(
                older_endpoint.clone(),
                genesis,
                RemotePlacement::Replica,
            )
            .unwrap(),
        PullLayerResult::AlreadyContained {
            current_layer_id,
            requested_layer_id,
            placement: RemotePlacement::Reference,
        } if current_layer_id == next && requested_layer_id == genesis
    ));
    assert_eq!(
        older_endpoint
            .layer_ancestry_records
            .load(Ordering::Relaxed),
        0,
        "an older Layer boundary must be classified from the acquired local prefix"
    );
    let layers = branches.fact_page(FactKind::Layer, None, 128).unwrap().0;
    assert_eq!(layers.len(), 2);
    let scopes = branches.layer_stack_scope_page(None, 512).unwrap();
    assert_eq!(scopes.records.len(), 1);
    assert_eq!(scopes.records[0].0.name.as_str(), "project");
    assert_eq!(scopes.records[0].1.through_layer_id, next);
    let branch_scopes = branches
        .branch_scope_page(Some(initialized.layer_stack_id), None, 512)
        .unwrap();
    assert_eq!(branch_scopes.records.len(), 1);
    assert_eq!(branch_scopes.records[0].0.name.as_str(), "main");
    assert!(matches!(
        branches.fork_branch(
            EntityName::new("main").unwrap(),
            LocalForkSource::Layer { layer_id: next },
        ),
        Err(StorageError::BranchNameConflict {
            layer_stack_id,
            existing_id,
            ..
        }) if layer_stack_id == initialized.layer_stack_id && existing_id == local
    ));
    assert!(matches!(
        branches
            .pull_layer(authority.clone(), next, RemotePlacement::Replica)
            .unwrap(),
        PullLayerResult::ModeChanged { .. }
    ));
    let unavailable = Arc::new(CountingEndpoint::new(authority.clone()));
    unavailable.fail_all.store(1, Ordering::Relaxed);
    let mut differences = Vec::new();
    branches
        .visit_layer_diff(unavailable.clone(), genesis, next, |entry| {
            differences.push(entry);
            Ok(())
        })
        .expect("Replica Layer Diff must be offline");
    assert!(!differences.is_empty());
    assert_eq!(unavailable.calls.load(Ordering::Relaxed), 0);

    drop(branches);
    drop(authority);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn replica_layer_prefix_keeps_deleted_historical_objects_offline() {
    let root = temp("historical-layer-offline");
    let source_dir = root.join("source");
    std::fs::create_dir_all(&source_dir).unwrap();
    std::fs::write(source_dir.join("old.dat"), b"historical").unwrap();
    let authority = Arc::new(LayerStackStore::create(root.join("authority.sqlite")).unwrap());
    let first = authority
        .initialize_layerstack(
            EntityName::new("project").unwrap(),
            LayerStackInitialization::Directory(source_dir),
        )
        .unwrap()
        .genesis_layer_id;
    let source = BranchStore::create(root.join("source.sqlite"), authority.store_id()).unwrap();
    source
        .pull_layer(authority.clone(), first, RemotePlacement::Reference)
        .unwrap();
    let branch = source
        .fork_branch(
            EntityName::new("main").unwrap(),
            LocalForkSource::Layer { layer_id: first },
        )
        .unwrap();
    commit_once(
        &source,
        authority.clone(),
        branch,
        None,
        &[remove("old.dat"), write("current.dat", b"current")],
    );
    source.push_branch(authority.clone(), branch).unwrap();
    let AuthorityAddResult::Added { layer_id: second } = authority.add_layer(branch).unwrap()
    else {
        panic!("second Layer")
    };

    let replica = BranchStore::create(root.join("replica.sqlite"), authority.store_id()).unwrap();
    replica
        .pull_layer(authority.clone(), second, RemotePlacement::Replica)
        .unwrap();
    let first_root = replica.layer(first).unwrap().unwrap().root_id;
    let second_root = replica.layer(second).unwrap().unwrap().root_id;
    assert!(replica.root_complete(first_root).unwrap());
    assert!(replica.root_complete(second_root).unwrap());
    let unavailable = Arc::new(CountingEndpoint::new(authority.clone()));
    unavailable.fail_all.store(1, Ordering::Relaxed);
    let reader = replica
        .snapshot_reader(unavailable.clone(), first_root)
        .expect("historical Layer reader must be offline");
    let mut historical = Vec::new();
    layerfs_content::filesystem::stream(
        &layerfs_storage::CoreReader(&reader),
        first_root,
        &layerfs_content::CanonicalPath::new("old.dat").unwrap(),
        &mut historical,
    )
    .unwrap();
    assert_eq!(historical, b"historical");
    assert_eq!(unavailable.calls.load(Ordering::Relaxed), 0);

    drop(replica);
    drop(source);
    drop(authority);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn branch_pull_is_read_only_complete_history_and_push_sends_only_owned_suffix() {
    let root = temp("branch-history");
    std::fs::create_dir_all(&root).unwrap();
    let authority = Arc::new(LayerStackStore::create(root.join("authority.sqlite")).unwrap());
    let genesis = authority
        .initialize_layerstack(
            EntityName::new("project").unwrap(),
            LayerStackInitialization::Empty,
        )
        .unwrap()
        .genesis_layer_id;
    let source = BranchStore::create(root.join("source.sqlite"), authority.store_id()).unwrap();
    source
        .pull_layer(authority.clone(), genesis, RemotePlacement::Reference)
        .unwrap();
    let main = source
        .fork_branch(
            EntityName::new("main").unwrap(),
            LocalForkSource::Layer { layer_id: genesis },
        )
        .unwrap();
    let commits = commit_chain(&source, authority.clone(), main, 3);
    source.push_branch(authority.clone(), main).unwrap();

    let rollout = BranchStore::create(root.join("rollout.sqlite"), authority.store_id()).unwrap();
    assert!(matches!(
        rollout
            .pull_branch(
                authority.clone(),
                main,
                commits[1],
                RemotePlacement::Reference,
            )
            .unwrap(),
        PullBranchResult::Created { .. }
    ));
    assert!(rollout.branch_contains_commit(main, commits[0]).unwrap());
    assert_eq!(rollout.branch(main).unwrap().unwrap().id, main);
    assert!(matches!(
        rollout.commit_changes(authority.clone(), main, Some(commits[1]), &[]),
        Err(StorageError::ReadOnlyBranch(id)) if id == main
    ));
    assert!(matches!(
        rollout.push_branch(authority.clone(), main),
        Err(StorageError::ReadOnlyBranch(id)) if id == main
    ));
    assert!(matches!(
        rollout
            .pull_branch(
                authority.clone(),
                main,
                commits[2],
                RemotePlacement::Reference,
            )
            .unwrap(),
        PullBranchResult::Advanced {
            previous_commit_id,
            through_commit_id,
            ..
        } if previous_commit_id == commits[1] && through_commit_id == commits[2]
    ));
    assert!(matches!(
        rollout
            .pull_branch(
                authority.clone(),
                main,
                commits[0],
                RemotePlacement::Replica,
            )
            .unwrap(),
        PullBranchResult::AlreadyContained {
            current_commit_id,
            requested_commit_id,
            placement: RemotePlacement::Reference,
        } if current_commit_id == commits[2] && requested_commit_id == commits[0]
    ));
    assert!(matches!(
        rollout
            .pull_branch(
                authority.clone(),
                main,
                commits[2],
                RemotePlacement::Replica,
            )
            .unwrap(),
        PullBranchResult::ModeChanged { .. }
    ));
    for commit in &commits {
        let root_id = rollout.commit(*commit).unwrap().unwrap().root_id;
        assert!(rollout.root_complete(root_id).unwrap());
    }
    assert!(rollout
        .root_complete(authority.layer(genesis).unwrap().unwrap().root_id)
        .unwrap());
    let historical_unavailable = Arc::new(CountingEndpoint::new(authority.clone()));
    historical_unavailable.fail_all.store(1, Ordering::Relaxed);
    for commit in &commits {
        let root_id = rollout.commit(*commit).unwrap().unwrap().root_id;
        let reader = rollout
            .snapshot_reader(historical_unavailable.clone(), root_id)
            .expect("every historical Commit root must be offline");
        assert!(reader.read_object(root_id).is_ok());
    }
    assert_eq!(historical_unavailable.calls.load(Ordering::Relaxed), 0);
    assert!(matches!(
        rollout
            .pull_branch(
                authority.clone(),
                main,
                commits[2],
                RemotePlacement::Reference,
            )
            .unwrap(),
        PullBranchResult::ModeChanged {
            previous: RemotePlacement::Replica,
            ..
        }
    ));
    let unavailable = Arc::new(CountingEndpoint::new(authority.clone()));
    unavailable.fail_all.store(1, Ordering::Relaxed);
    let pinned = rollout
        .pin_branch(unavailable.clone(), main)
        .expect("retained complete receipt must keep Reference offline");
    assert!(pinned.reader.read_object(pinned.root).is_ok());
    assert_eq!(unavailable.calls.load(Ordering::Relaxed), 0);
    let layer_scope = rollout.layer_stack_scope_page(None, 512).unwrap();
    assert_eq!(layer_scope.records.len(), 1);
    assert_eq!(
        layer_scope.records[0].1.serving_mode,
        RemotePlacement::Replica,
        "a Branch Replica-to-Reference policy change must not weaken its required Layer prefix"
    );

    take_transfer_receipts();
    let objects_before_fork = rollout.inventory_page(None, 512).unwrap().entries.len();
    let child = rollout
        .fork_branch(
            EntityName::new("child").unwrap(),
            LocalForkSource::Branch {
                branch_id: main,
                commit_id: commits[1],
            },
        )
        .unwrap();
    assert_eq!(
        rollout.inventory_page(None, 512).unwrap().entries.len(),
        objects_before_fork
    );
    assert!(take_transfer_receipts().is_empty());
    let CommitOutcome::Created {
        commit_id: child_commit,
        ..
    } = rollout
        .commit_changes(
            authority.clone(),
            child,
            Some(commits[1]),
            &[ContentChange::Write {
                path: "child".into(),
                bytes: b"child".to_vec(),
                mode: 0o644,
            }],
        )
        .unwrap()
    else {
        panic!("child Commit")
    };
    take_transfer_receipts();
    rollout.push_branch(authority.clone(), child).unwrap();
    let totals = receipt_totals(&take_transfer_receipts());
    assert_eq!(totals[&FactKind::Commit].announced_ids, 1);
    assert_eq!(
        authority.branch(child).unwrap().unwrap().head_commit_id,
        Some(child_commit)
    );
    assert!(authority.commit(commits[0]).unwrap().is_some());

    let mixed = BranchStore::create(root.join("mixed.sqlite"), authority.store_id()).unwrap();
    mixed
        .pull_layer(authority.clone(), genesis, RemotePlacement::Replica)
        .unwrap();
    mixed
        .pull_branch(
            authority.clone(),
            main,
            commits[2],
            RemotePlacement::Reference,
        )
        .unwrap();
    let genesis_root = authority.layer(genesis).unwrap().unwrap().root_id;
    rusqlite::Connection::open(root.join("mixed.sqlite"))
        .unwrap()
        .execute(
            "DELETE FROM objects WHERE object_id=?1",
            [genesis_root.as_bytes().as_slice()],
        )
        .unwrap();
    let endpoint = Arc::new(CountingEndpoint::new(authority.clone()));
    assert!(mixed
        .visit_branch_layer_diff(endpoint.clone(), main, genesis, |_| Ok(()))
        .is_err());
    assert_eq!(
        endpoint.object_calls.load(Ordering::Relaxed),
        0,
        "a missing receipted Layer object must not fall back through a Reference Branch"
    );

    drop(mixed);
    drop(rollout);
    drop(source);
    drop(authority);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn branch_pull_imports_inherited_ancestry_and_origin_facts() {
    let root = temp("inherited-history");
    std::fs::create_dir_all(&root).unwrap();
    let authority = Arc::new(LayerStackStore::create(root.join("authority.sqlite")).unwrap());
    let genesis = authority
        .initialize_layerstack(
            EntityName::new("project").unwrap(),
            LayerStackInitialization::Empty,
        )
        .unwrap()
        .genesis_layer_id;
    let source = BranchStore::create(root.join("source.sqlite"), authority.store_id()).unwrap();
    source
        .pull_layer(authority.clone(), genesis, RemotePlacement::Reference)
        .unwrap();
    let main = source
        .fork_branch(
            EntityName::new("main").unwrap(),
            LocalForkSource::Layer { layer_id: genesis },
        )
        .unwrap();
    let main_commits = commit_chain(&source, authority.clone(), main, 3);
    source.push_branch(authority.clone(), main).unwrap();
    let child = source
        .fork_branch(
            EntityName::new("child").unwrap(),
            LocalForkSource::Branch {
                branch_id: main,
                commit_id: main_commits[1],
            },
        )
        .unwrap();
    let CommitOutcome::Created {
        commit_id: child_commit,
        ..
    } = source
        .commit_changes(
            authority.clone(),
            child,
            Some(main_commits[1]),
            &[ContentChange::Write {
                path: "child".into(),
                bytes: b"child".to_vec(),
                mode: 0o644,
            }],
        )
        .unwrap()
    else {
        panic!("child Commit")
    };
    source.push_branch(authority.clone(), child).unwrap();

    let replica = BranchStore::create(root.join("replica.sqlite"), authority.store_id()).unwrap();
    replica
        .pull_branch(
            authority.clone(),
            child,
            child_commit,
            RemotePlacement::Replica,
        )
        .unwrap();
    assert!(replica
        .branch_contains_commit(child, main_commits[0])
        .unwrap());
    assert!(replica
        .branch_contains_commit(child, main_commits[1])
        .unwrap());
    assert!(!replica
        .branch_contains_commit(child, main_commits[2])
        .unwrap());
    assert!(replica.branch_contains_commit(child, child_commit).unwrap());
    assert!(replica.branch_fact(main).unwrap().is_some());
    assert!(replica.branch(main).unwrap().is_none());
    assert_eq!(
        replica
            .layer_stack_scope_page(None, 512)
            .unwrap()
            .records
            .len(),
        1,
        "Branch Pull must publish its required named LayerStack prefix"
    );
    assert_eq!(replica.layer(genesis).unwrap().unwrap().id, genesis);
    replica
        .fork_branch(
            EntityName::new("from-required-layer").unwrap(),
            LocalForkSource::Layer { layer_id: genesis },
        )
        .expect("required Branch ancestry Layer must be a visible local Fork source");
    for commit in [main_commits[0], main_commits[1], child_commit] {
        let root_id = replica.commit(commit).unwrap().unwrap().root_id;
        assert!(replica.root_complete(root_id).unwrap());
    }

    drop(replica);
    drop(source);
    drop(authority);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn incremental_pull_requests_only_suffix_and_interruption_hides_scope() {
    let root = temp("incremental");
    std::fs::create_dir_all(&root).unwrap();
    let authority = Arc::new(LayerStackStore::create(root.join("authority.sqlite")).unwrap());
    let genesis = authority
        .initialize_layerstack(
            EntityName::new("project").unwrap(),
            LayerStackInitialization::Empty,
        )
        .unwrap()
        .genesis_layer_id;
    let source = BranchStore::create(root.join("source.sqlite"), authority.store_id()).unwrap();
    source
        .pull_layer(authority.clone(), genesis, RemotePlacement::Reference)
        .unwrap();
    let main = source
        .fork_branch(
            EntityName::new("main").unwrap(),
            LocalForkSource::Layer { layer_id: genesis },
        )
        .unwrap();
    let commits = commit_chain(&source, authority.clone(), main, 6);
    source.push_branch(authority.clone(), main).unwrap();

    let rollout = BranchStore::create(root.join("rollout.sqlite"), authority.store_id()).unwrap();
    rollout
        .pull_branch(
            authority.clone(),
            main,
            commits[2],
            RemotePlacement::Reference,
        )
        .unwrap();
    let endpoint = Arc::new(CountingEndpoint::new(authority.clone()));
    assert!(matches!(
        rollout
            .pull_branch(
                endpoint.clone(),
                main,
                commits[5],
                RemotePlacement::Reference,
            )
            .unwrap(),
        PullBranchResult::Advanced { .. }
    ));
    assert_eq!(endpoint.commit_ancestry_records.load(Ordering::Relaxed), 3);

    endpoint.commit_ancestry_records.store(0, Ordering::Relaxed);
    endpoint.commit_history_calls.store(0, Ordering::Relaxed);
    assert!(matches!(
        rollout
            .pull_branch(endpoint.clone(), main, commits[2], RemotePlacement::Replica,)
            .unwrap(),
        PullBranchResult::AlreadyContained { .. }
    ));
    assert_eq!(
        endpoint.commit_ancestry_records.load(Ordering::Relaxed),
        0,
        "an older boundary must be classified from the acquired local history"
    );
    assert_eq!(endpoint.commit_history_calls.load(Ordering::Relaxed), 0);

    endpoint.commit_ancestry_records.store(0, Ordering::Relaxed);
    rollout
        .pull_branch(endpoint.clone(), main, commits[5], RemotePlacement::Replica)
        .unwrap();
    assert_eq!(endpoint.commit_ancestry_records.load(Ordering::Relaxed), 0);

    let interrupted =
        BranchStore::create(root.join("interrupted.sqlite"), authority.store_id()).unwrap();
    let failing = Arc::new(CountingEndpoint::new(authority.clone()));
    failing.fail_objects.store(1, Ordering::Relaxed);
    assert!(matches!(
        interrupted.pull_branch(failing, main, commits[5], RemotePlacement::Replica,),
        Err(StorageError::Unavailable)
    ));
    assert!(interrupted.branch(main).unwrap().is_none());
    assert!(interrupted.layer(genesis).unwrap().is_none());
    assert!(interrupted
        .layer_stack_scope_page(None, 512)
        .unwrap()
        .records
        .is_empty());

    let unavailable = Arc::new(CountingEndpoint::new(authority.clone()));
    unavailable.fail_all.store(1, Ordering::Relaxed);
    let pinned = rollout
        .pin_branch(unavailable.clone(), main)
        .expect("Replica pin is offline");
    assert_eq!(unavailable.calls.load(Ordering::Relaxed), 0);
    assert!(pinned.reader.read_object(pinned.root).is_ok());
    rusqlite::Connection::open(root.join("rollout.sqlite"))
        .unwrap()
        .execute(
            "DELETE FROM objects WHERE object_id=?1",
            [pinned.root.as_bytes().as_slice()],
        )
        .unwrap();
    assert!(matches!(
        pinned.reader.read_object(pinned.root),
        Err(StorageError::Integrity("complete local closure"))
    ));
    assert_eq!(unavailable.calls.load(Ordering::Relaxed), 0);

    drop(interrupted);
    drop(rollout);
    drop(source);
    drop(authority);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn branch_pull_crosses_more_than_512_commits_without_truncation() {
    let root = temp("long-history");
    std::fs::create_dir_all(&root).unwrap();
    let authority = Arc::new(LayerStackStore::create(root.join("authority.sqlite")).unwrap());
    let genesis = authority
        .initialize_layerstack(
            EntityName::new("project").unwrap(),
            LayerStackInitialization::Empty,
        )
        .unwrap()
        .genesis_layer_id;
    let source = BranchStore::create(root.join("source.sqlite"), authority.store_id()).unwrap();
    source
        .pull_layer(authority.clone(), genesis, RemotePlacement::Reference)
        .unwrap();
    let main = source
        .fork_branch(
            EntityName::new("main").unwrap(),
            LocalForkSource::Layer { layer_id: genesis },
        )
        .unwrap();
    let commits = commit_chain(&source, authority.clone(), main, 520);
    source.push_branch(authority.clone(), main).unwrap();

    let rollout = BranchStore::create(root.join("rollout.sqlite"), authority.store_id()).unwrap();
    rollout
        .pull_branch(
            authority.clone(),
            main,
            *commits.last().unwrap(),
            RemotePlacement::Reference,
        )
        .unwrap();
    assert!(rollout.branch_contains_commit(main, commits[0]).unwrap());
    assert!(rollout.commit(commits[0]).unwrap().is_some());
    assert!(rollout.commit(commits[519]).unwrap().is_some());

    drop(rollout);
    drop(source);
    drop(authority);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn endpoint_point_reads_echo_keys_and_equal_pull_compares_the_full_layer_fact() {
    let root = temp("endpoint-identity");
    std::fs::create_dir_all(&root).unwrap();
    let authority = Arc::new(LayerStackStore::create(root.join("authority.sqlite")).unwrap());
    let first = authority
        .initialize_layerstack(
            EntityName::new("first").unwrap(),
            LayerStackInitialization::Empty,
        )
        .unwrap()
        .genesis_layer_id;
    let second = authority
        .initialize_layerstack(
            EntityName::new("second").unwrap(),
            LayerStackInitialization::Empty,
        )
        .unwrap()
        .genesis_layer_id;

    let receiver = BranchStore::create(root.join("receiver.sqlite"), authority.store_id()).unwrap();
    receiver
        .pull_layer(authority.clone(), first, RemotePlacement::Reference)
        .unwrap();
    let endpoint = Arc::new(CountingEndpoint::new(authority.clone()));
    let mut changed_provenance = authority.layer(first).unwrap().unwrap();
    changed_provenance.source_branch_id = Some(BranchId::new());
    changed_provenance.source_commit_id = Some(CommitId::derive(
        changed_provenance.root_id,
        None,
        changed_provenance.id,
    ));
    *endpoint.layer_override.lock().unwrap() = Some(changed_provenance);
    assert!(matches!(
        receiver.pull_layer(endpoint.clone(), first, RemotePlacement::Reference),
        Err(StorageError::Integrity(_))
    ));
    let point_page =
        BranchStore::create(root.join("point-page.sqlite"), authority.store_id()).unwrap();
    assert!(matches!(
        point_page.pull_layer(endpoint.clone(), first, RemotePlacement::Reference),
        Err(StorageError::Integrity(_))
    ));
    assert!(point_page
        .layer_stack_scope_page(None, 512)
        .unwrap()
        .records
        .is_empty());

    let wrong_key =
        BranchStore::create(root.join("wrong-key.sqlite"), authority.store_id()).unwrap();
    *endpoint.layer_override.lock().unwrap() = authority.layer(second).unwrap();
    assert!(matches!(
        wrong_key.pull_layer(endpoint, first, RemotePlacement::Reference),
        Err(StorageError::Integrity(_))
    ));

    let source = BranchStore::create(root.join("source.sqlite"), authority.store_id()).unwrap();
    source
        .pull_layer(authority.clone(), first, RemotePlacement::Reference)
        .unwrap();
    let main = source
        .fork_branch(
            EntityName::new("main").unwrap(),
            LocalForkSource::Layer { layer_id: first },
        )
        .unwrap();
    let commit = commit_once(
        &source,
        authority.clone(),
        main,
        None,
        &[write("branch", b"one")],
    );
    source.push_branch(authority.clone(), main).unwrap();
    let wrong_branch =
        BranchStore::create(root.join("wrong-branch.sqlite"), authority.store_id()).unwrap();
    let endpoint = Arc::new(CountingEndpoint::new(authority.clone()));
    let mut substituted = authority.branch(main).unwrap().unwrap();
    substituted.id = BranchId::new();
    *endpoint.branch_override.lock().unwrap() = Some(substituted);
    assert!(matches!(
        wrong_branch.pull_branch(endpoint, main, commit, RemotePlacement::Reference),
        Err(StorageError::Integrity(_))
    ));
    let wrong_commit =
        BranchStore::create(root.join("wrong-commit.sqlite"), authority.store_id()).unwrap();
    let endpoint = Arc::new(CountingEndpoint::new(authority.clone()));
    let actual = authority.commit(commit).unwrap().unwrap();
    *endpoint.commit_override.lock().unwrap() = Some(CommitRecord {
        id: CommitId::derive(actual.root_id, actual.parent_commit_id, second),
        base_layer_id: second,
        ..actual
    });
    assert!(matches!(
        wrong_commit.pull_branch(endpoint, main, commit, RemotePlacement::Reference),
        Err(StorageError::Integrity(_))
    ));
    let wrong_page =
        BranchStore::create(root.join("wrong-page.sqlite"), authority.store_id()).unwrap();
    let endpoint = Arc::new(CountingEndpoint::new(authority.clone()));
    *endpoint.commit_override.lock().unwrap() = Some(CommitRecord {
        root_id: layerfs_content::ObjectId::for_bytes(b"point-page-mismatch"),
        ..actual
    });
    assert!(matches!(
        wrong_page.pull_branch(endpoint, main, commit, RemotePlacement::Reference),
        Err(StorageError::Integrity(_))
    ));

    drop(wrong_page);
    drop(wrong_commit);
    drop(wrong_branch);
    drop(source);
    drop(point_page);
    drop(wrong_key);
    drop(receiver);
    drop(authority);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn push_never_uses_authority_to_mask_a_missing_receipted_local_object() {
    let root = temp("push-complete-policy");
    std::fs::create_dir_all(&root).unwrap();
    let authority = Arc::new(LayerStackStore::create(root.join("authority.sqlite")).unwrap());
    let genesis = authority
        .initialize_layerstack(
            EntityName::new("project").unwrap(),
            LayerStackInitialization::Empty,
        )
        .unwrap()
        .genesis_layer_id;
    let branches = BranchStore::create(root.join("branch.sqlite"), authority.store_id()).unwrap();
    branches
        .pull_layer(authority.clone(), genesis, RemotePlacement::Replica)
        .unwrap();
    let branch = branches
        .fork_branch(
            EntityName::new("main").unwrap(),
            LocalForkSource::Layer { layer_id: genesis },
        )
        .unwrap();
    commit_once(
        &branches,
        authority.clone(),
        branch,
        None,
        &[write("new", b"new")],
    );
    let head_root = branches.branch_root(branch).unwrap();
    assert!(branches.root_complete(head_root).unwrap());
    let genesis_root = authority.layer(genesis).unwrap().unwrap().root_id;
    let head_ids = layerfs_storage::dependency_order(&branches, head_root).unwrap();
    let genesis_ids = layerfs_storage::dependency_order(&branches, genesis_root)
        .unwrap()
        .into_iter()
        .collect::<std::collections::BTreeSet<_>>();
    let shared = head_ids
        .into_iter()
        .find(|id| *id != head_root && genesis_ids.contains(id))
        .expect("new snapshot must reuse at least one complete base object");
    rusqlite::Connection::open(root.join("branch.sqlite"))
        .unwrap()
        .execute(
            "DELETE FROM objects WHERE object_id=?1",
            [shared.as_bytes().as_slice()],
        )
        .unwrap();
    assert!(branches.push_branch(authority.clone(), branch).is_err());
    assert!(authority.branch(branch).unwrap().is_none());

    drop(branches);
    drop(authority);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn reconciliation_preserves_branch_layer_and_working_tree_as_distinct_choices() {
    let root = temp("three-resolution-choices");
    std::fs::create_dir_all(&root).unwrap();
    let authority = Arc::new(LayerStackStore::create(root.join("authority.sqlite")).unwrap());
    let genesis = authority
        .initialize_layerstack(
            EntityName::new("project").unwrap(),
            LayerStackInitialization::Empty,
        )
        .unwrap()
        .genesis_layer_id;
    let branches = BranchStore::create(root.join("branch.sqlite"), authority.store_id()).unwrap();
    branches
        .pull_layer(authority.clone(), genesis, RemotePlacement::Replica)
        .unwrap();
    let accepted = branches
        .fork_branch(
            EntityName::new("accepted").unwrap(),
            LocalForkSource::Layer { layer_id: genesis },
        )
        .unwrap();
    branches
        .commit_changes(
            authority.clone(),
            accepted,
            None,
            &[write("same", b"layer")],
        )
        .unwrap();
    branches.push_branch(authority.clone(), accepted).unwrap();
    let AuthorityAddResult::Added { layer_id: current } = authority.add_layer(accepted).unwrap()
    else {
        panic!("accepted Add")
    };
    branches
        .pull_layer(authority.clone(), current, RemotePlacement::Replica)
        .unwrap();

    for (name, choice, expected) in [
        (
            "choose-branch",
            ReconcileChoice::Branch,
            b"branch".as_slice(),
        ),
        ("choose-layer", ReconcileChoice::Layer, b"layer".as_slice()),
        (
            "choose-working",
            ReconcileChoice::WorkingTree,
            b"working".as_slice(),
        ),
    ] {
        let branch = branches
            .fork_branch(
                EntityName::new(name).unwrap(),
                LocalForkSource::Layer { layer_id: genesis },
            )
            .unwrap();
        branches
            .commit_changes(authority.clone(), branch, None, &[write("same", b"branch")])
            .unwrap();
        branches.push_branch(authority.clone(), branch).unwrap();
        let prepared = branches
            .prepare_reconciliation(authority.clone(), branch, current)
            .unwrap();
        assert!(!prepared.conflicts.is_empty());
        let reader = branches
            .snapshot_reader(authority.clone(), prepared.root_id)
            .unwrap();
        let working = layerfs_storage::apply_changes(
            &reader,
            prepared.root_id,
            &[write("same", b"working")],
            [11; 32],
        )
        .unwrap();
        branches
            .commit_reconciliation(
                authority.clone(),
                &prepared,
                working,
                &vec![choice; prepared.conflicts.len()],
            )
            .unwrap();
        assert_eq!(
            read_file(&branches, authority.clone(), branch, "same"),
            expected
        );
    }

    drop(branches);
    drop(authority);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn every_conflict_kind_applies_exact_branch_layer_and_working_tree_roots() {
    use layerfs_storage::ReconcileConflictKind;

    let root = temp("all-resolution-kinds");
    std::fs::create_dir_all(&root).unwrap();
    let authority = Arc::new(LayerStackStore::create(root.join("authority.sqlite")).unwrap());
    let branches = BranchStore::create(root.join("branch.sqlite"), authority.store_id()).unwrap();

    for (serial, kind, branch_changes, layer_changes, working_changes) in [
        (
            1_u8,
            ReconcileConflictKind::Content,
            vec![write("node", b"branch")],
            vec![write("node", b"layer")],
            vec![write("node", b"working")],
        ),
        (
            2,
            ReconcileConflictKind::Type,
            vec![remove("node"), mkdir("node")],
            vec![remove("node"), symlink("node", b"layer")],
            vec![remove("node"), write("node", b"working")],
        ),
        (
            3,
            ReconcileConflictKind::Directory,
            vec![remove("node")],
            vec![write("node", b"layer")],
            vec![symlink("node", b"working")],
        ),
        (
            4,
            ReconcileConflictKind::HardLink,
            vec![hard_link("node", "branch-link")],
            vec![hard_link("node", "layer-link")],
            vec![hard_link("node", "working-link")],
        ),
    ] {
        let source = root.join(format!("base-{serial}"));
        std::fs::create_dir_all(&source).unwrap();
        std::fs::write(source.join("node"), b"base").unwrap();
        let initialized = authority
            .initialize_layerstack(
                EntityName::new(format!("project-{serial}")).unwrap(),
                LayerStackInitialization::Directory(source),
            )
            .unwrap();
        let genesis = initialized.genesis_layer_id;
        branches
            .pull_layer(authority.clone(), genesis, RemotePlacement::Replica)
            .unwrap();
        let accepted = branches
            .fork_branch(
                EntityName::new(format!("accepted-{serial}")).unwrap(),
                LocalForkSource::Layer { layer_id: genesis },
            )
            .unwrap();
        commit_once(&branches, authority.clone(), accepted, None, &layer_changes);

        let choices = [
            ("branch", ReconcileChoice::Branch),
            ("layer", ReconcileChoice::Layer),
            ("working", ReconcileChoice::WorkingTree),
        ];
        let mut stale = Vec::new();
        for (label, choice) in choices {
            let branch = branches
                .fork_branch(
                    EntityName::new(format!("{label}-{serial}")).unwrap(),
                    LocalForkSource::Layer { layer_id: genesis },
                )
                .unwrap();
            commit_once(&branches, authority.clone(), branch, None, &branch_changes);
            branches.push_branch(authority.clone(), branch).unwrap();
            stale.push((branch, choice));
        }
        branches.push_branch(authority.clone(), accepted).unwrap();
        let AuthorityAddResult::Added { layer_id: current } =
            authority.add_layer(accepted).unwrap()
        else {
            panic!("accepted Add")
        };
        branches
            .pull_layer(authority.clone(), current, RemotePlacement::Replica)
            .unwrap();

        for (branch, choice) in stale {
            let prepared = branches
                .prepare_reconciliation(authority.clone(), branch, current)
                .unwrap();
            assert!(
                prepared
                    .conflicts
                    .iter()
                    .any(|conflict| conflict.kind == kind),
                "missing {kind:?} conflict"
            );
            let reader = branches
                .snapshot_reader(authority.clone(), prepared.root_id)
                .unwrap();
            let working = layerfs_storage::apply_changes(
                &reader,
                prepared.root_id,
                &working_changes,
                [serial; 32],
            )
            .unwrap();
            let expected = match choice {
                ReconcileChoice::Branch => prepared.branch_root,
                ReconcileChoice::Layer => prepared.layer_root,
                ReconcileChoice::WorkingTree => working.root_id,
            };
            branches
                .commit_reconciliation(
                    authority.clone(),
                    &prepared,
                    working,
                    &vec![choice; prepared.conflicts.len()],
                )
                .unwrap();
            assert_logically_equal(
                &branches,
                authority.clone(),
                branches.branch_root(branch).unwrap(),
                expected,
                &format!("{kind:?}/{choice:?}"),
            );
        }
    }

    drop(branches);
    drop(authority);
    std::fs::remove_dir_all(root).unwrap();
}

fn commit_chain(
    store: &BranchStore,
    authority: Arc<LayerStackStore>,
    branch: layerfs_storage::BranchId,
    count: usize,
) -> Vec<layerfs_storage::CommitId> {
    let mut head = None;
    (0..count)
        .map(|index| {
            let outcome = store
                .commit_changes(
                    authority.clone(),
                    branch,
                    head,
                    &[ContentChange::Write {
                        path: format!("file-{index}"),
                        bytes: vec![index as u8; 64],
                        mode: 0o644,
                    }],
                )
                .unwrap();
            let CommitOutcome::Created { commit_id, .. } = outcome else {
                panic!("Commit")
            };
            head = Some(commit_id);
            commit_id
        })
        .collect()
}

fn write(path: &str, bytes: &[u8]) -> ContentChange {
    ContentChange::Write {
        path: path.to_owned(),
        bytes: bytes.to_vec(),
        mode: 0o644,
    }
}

fn remove(path: &str) -> ContentChange {
    ContentChange::Remove {
        path: path.to_owned(),
    }
}

fn mkdir(path: &str) -> ContentChange {
    ContentChange::Mkdir {
        path: path.to_owned(),
        mode: 0o755,
    }
}

fn symlink(path: &str, target: &[u8]) -> ContentChange {
    ContentChange::Symlink {
        path: path.to_owned(),
        target: target.to_vec(),
    }
}

fn hard_link(source: &str, target: &str) -> ContentChange {
    ContentChange::HardLink {
        source: source.to_owned(),
        target: target.to_owned(),
    }
}

fn commit_once(
    store: &BranchStore,
    authority: Arc<LayerStackStore>,
    branch: BranchId,
    expected: Option<CommitId>,
    changes: &[ContentChange],
) -> CommitId {
    let CommitOutcome::Created { commit_id, .. } = store
        .commit_changes(authority, branch, expected, changes)
        .unwrap()
    else {
        panic!("Commit")
    };
    commit_id
}

fn assert_logically_equal(
    store: &BranchStore,
    authority: Arc<LayerStackStore>,
    actual: layerfs_content::ObjectId,
    expected: layerfs_content::ObjectId,
    context: &str,
) {
    assert_eq!(actual, expected, "{context} canonical root mismatch");
    let reader = store.snapshot_reader(authority, actual).unwrap();
    let mut differences = Vec::new();
    layerfs_content::filesystem::diff_roots(
        &layerfs_storage::CoreReader(&reader),
        actual,
        expected,
        |entry| {
            differences.push(entry);
            Ok(())
        },
    )
    .unwrap();
    assert!(
        differences.is_empty(),
        "{context} logical difference: {differences:?}"
    );
}

fn read_file(
    store: &BranchStore,
    authority: Arc<LayerStackStore>,
    branch: BranchId,
    path: &str,
) -> Vec<u8> {
    let pinned = store.pin_branch(authority, branch).unwrap();
    let mut bytes = Vec::new();
    layerfs_content::filesystem::stream(
        &layerfs_storage::CoreReader(&pinned.reader),
        pinned.root,
        &layerfs_content::CanonicalPath::new(path).unwrap(),
        &mut bytes,
    )
    .unwrap();
    bytes
}

fn temp(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "layerfs-v2-branch-{name}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
}

struct CountingEndpoint {
    inner: Arc<LayerStackStore>,
    calls: AtomicUsize,
    object_calls: AtomicUsize,
    commit_ancestry_records: AtomicUsize,
    commit_history_calls: AtomicUsize,
    layer_ancestry_records: AtomicUsize,
    fail_objects: AtomicUsize,
    fail_all: AtomicUsize,
    layer_override: Mutex<Option<LayerRecord>>,
    branch_override: Mutex<Option<BranchRecord>>,
    commit_override: Mutex<Option<CommitRecord>>,
}

impl CountingEndpoint {
    fn new(inner: Arc<LayerStackStore>) -> Self {
        Self {
            inner,
            calls: AtomicUsize::new(0),
            object_calls: AtomicUsize::new(0),
            commit_ancestry_records: AtomicUsize::new(0),
            commit_history_calls: AtomicUsize::new(0),
            layer_ancestry_records: AtomicUsize::new(0),
            fail_objects: AtomicUsize::new(0),
            fail_all: AtomicUsize::new(0),
            layer_override: Mutex::new(None),
            branch_override: Mutex::new(None),
            commit_override: Mutex::new(None),
        }
    }

    fn enter(&self) -> layerfs_storage::Result<()> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        if self.fail_all.load(Ordering::Relaxed) != 0 {
            Err(StorageError::Unavailable)
        } else {
            Ok(())
        }
    }
}

impl ObjectSource for CountingEndpoint {
    fn read_object(&self, id: layerfs_content::ObjectId) -> layerfs_storage::Result<Vec<u8>> {
        self.enter()?;
        self.object_calls.fetch_add(1, Ordering::Relaxed);
        if self.fail_objects.load(Ordering::Relaxed) != 0 {
            return Err(StorageError::Unavailable);
        }
        self.inner.read_object(id)
    }

    fn read_objects(
        &self,
        ids: &[layerfs_content::ObjectId],
    ) -> layerfs_storage::Result<Vec<CanonicalObject>> {
        self.enter()?;
        self.object_calls.fetch_add(1, Ordering::Relaxed);
        if self.fail_objects.load(Ordering::Relaxed) != 0 {
            return Err(StorageError::Unavailable);
        }
        self.inner.read_objects(ids)
    }
}

impl LayerStackEndpoint for CountingEndpoint {
    fn store_id(&self) -> layerfs_storage::Result<StoreId> {
        self.enter()?;
        Ok(self.inner.store_id())
    }

    fn layer_stack_fact(
        &self,
        id: LayerStackId,
    ) -> layerfs_storage::Result<Option<LayerStackFact>> {
        self.enter()?;
        self.inner.layer_stack_fact(id)
    }

    fn layer_stack(&self, id: LayerStackId) -> layerfs_storage::Result<Option<LayerStackRecord>> {
        self.enter()?;
        self.inner.layer_stack(id)
    }

    fn layer(&self, id: LayerId) -> layerfs_storage::Result<Option<LayerRecord>> {
        self.enter()?;
        if let Some(record) = *self.layer_override.lock().unwrap() {
            return Ok(Some(record));
        }
        self.inner.layer(id)
    }

    fn branch_fact(&self, id: BranchId) -> layerfs_storage::Result<Option<BranchFact>> {
        self.enter()?;
        self.inner.branch_fact(id)
    }

    fn branch(&self, id: BranchId) -> layerfs_storage::Result<Option<BranchRecord>> {
        self.enter()?;
        if let Some(record) = self.branch_override.lock().unwrap().clone() {
            return Ok(Some(record));
        }
        self.inner.branch(id)
    }

    fn commit(&self, id: CommitId) -> layerfs_storage::Result<Option<CommitRecord>> {
        self.enter()?;
        if let Some(record) = *self.commit_override.lock().unwrap() {
            return Ok(Some(record));
        }
        self.inner.commit(id)
    }

    fn layer_prefix_page(
        &self,
        through: LayerId,
        cursor: Option<LayerId>,
        limit: u16,
    ) -> layerfs_storage::Result<LayerPrefixPage> {
        self.enter()?;
        self.inner.layer_prefix_page(through, cursor, limit)
    }

    fn layer_ancestry_page(
        &self,
        through: LayerId,
        stop: Option<LayerId>,
        cursor: Option<LayerId>,
        limit: u16,
    ) -> layerfs_storage::Result<LayerPrefixPage> {
        self.enter()?;
        let page = self
            .inner
            .layer_ancestry_page(through, stop, cursor, limit)?;
        self.layer_ancestry_records
            .fetch_add(page.records.len(), Ordering::Relaxed);
        Ok(page)
    }

    fn commit_history_page(
        &self,
        branch: BranchId,
        through: CommitId,
        cursor: Option<CommitId>,
        limit: u16,
    ) -> layerfs_storage::Result<CommitHistoryPage> {
        self.enter()?;
        self.commit_history_calls.fetch_add(1, Ordering::Relaxed);
        self.inner
            .commit_history_page(branch, through, cursor, limit)
    }

    fn commit_ancestry_page(
        &self,
        through: CommitId,
        stop: Option<CommitId>,
        cursor: Option<CommitId>,
        limit: u16,
    ) -> layerfs_storage::Result<CommitHistoryPage> {
        self.enter()?;
        let page = self
            .inner
            .commit_ancestry_page(through, stop, cursor, limit)?;
        self.commit_ancestry_records
            .fetch_add(page.records.len(), Ordering::Relaxed);
        Ok(page)
    }

    fn owned_commit_page(
        &self,
        branch: BranchId,
        through: CommitId,
        cursor: Option<CommitId>,
        limit: u16,
    ) -> layerfs_storage::Result<CommitHistoryPage> {
        self.enter()?;
        self.inner.owned_commit_page(branch, through, cursor, limit)
    }

    fn missing_objects(
        &self,
        ids: &[layerfs_content::ObjectId],
    ) -> layerfs_storage::Result<MissingBitmap> {
        self.enter()?;
        self.inner.missing_objects(ids)
    }

    fn missing_facts(&self, facts: &[Fact]) -> layerfs_storage::Result<MissingBitmap> {
        self.enter()?;
        self.inner.missing_facts(facts)
    }

    fn admit_objects(
        &self,
        objects: &[CanonicalObject],
    ) -> layerfs_storage::Result<AdmissionSetReceipt> {
        self.enter()?;
        self.inner.admit_objects(objects)
    }

    fn admit_facts(&self, facts: &[Fact]) -> layerfs_storage::Result<AdmissionSetReceipt> {
        self.enter()?;
        self.inner.admit_facts(facts)
    }

    fn publish_branch(
        &self,
        branch: &BranchRecord,
        observed: Option<CommitId>,
    ) -> layerfs_storage::Result<PushResult> {
        self.enter()?;
        self.inner.publish_branch(branch, observed)
    }

    fn add_layer(&self, branch: BranchId) -> layerfs_storage::Result<AuthorityAddResult> {
        self.enter()?;
        self.inner.add_layer(branch)
    }
}
