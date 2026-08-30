use layerfs_layerstack_store::LayerStackStore;
use layerfs_storage::{
    AuthorityAddResult, BranchId, BranchRecord, CanonicalObject, CommitId, CommitRecord,
    EntityName, Fact, InitializeLayerStackResult, LayerId, LayerPrefixPage, LayerStackEndpoint,
    LayerStackInitialization, PushResult, StorageError,
};

const HISTORY: usize = 520;

#[test]
fn exact_pages_remain_bounded_and_pinned_after_the_authority_advances() {
    let path = temp("history-pages");
    let store = LayerStackStore::create(&path).unwrap();
    let initialized = initialize(&store, "api-server");
    let mut base_layer_id = initialized.genesis_layer_id;
    let branch_id = BranchId::new();
    let branch_name = EntityName::new("main").unwrap();
    let mut parent_commit_id = None;
    let mut layers = vec![base_layer_id];
    let mut commits = Vec::new();

    for serial in 0..HISTORY as u64 {
        let root_id = admit_leaf(&store, serial);
        let commit = CommitRecord {
            id: CommitId::derive(root_id, parent_commit_id, base_layer_id),
            root_id,
            parent_commit_id,
            base_layer_id,
        };
        admit_commit(&store, commit);
        let branch = BranchRecord {
            id: branch_id,
            layer_stack_id: initialized.layer_stack_id,
            name: branch_name.clone(),
            base_layer_id,
            head_commit_id: Some(commit.id),
            forked_from_layer_id: Some(initialized.genesis_layer_id),
            forked_from_branch_id: None,
            forked_from_commit_id: None,
        };
        let published = LayerStackEndpoint::publish_branch(&store, &branch, parent_commit_id)
            .expect("publish exact owned suffix");
        if let Some(previous) = parent_commit_id {
            assert_eq!(
                published,
                PushResult::Advanced {
                    previous,
                    commit_id: commit.id,
                }
            );
        } else {
            assert_eq!(
                published,
                PushResult::Created {
                    commit_id: commit.id,
                }
            );
        }
        let AuthorityAddResult::Added { layer_id } = store.add_layer(branch_id).unwrap() else {
            panic!("each distinct root must create the next Layer")
        };
        commits.push(commit);
        layers.push(layer_id);
        parent_commit_id = Some(commit.id);
        base_layer_id = layer_id;
    }

    let pinned_layer_id = layers[400];
    let pinned_commit_id = commits[399].id;
    assert_ne!(
        store
            .layer_stack(initialized.layer_stack_id)
            .unwrap()
            .unwrap()
            .head_layer_id,
        pinned_layer_id
    );

    let full_prefix = collect_layers(&store, *layers.last().unwrap());
    assert_eq!(full_prefix.len(), HISTORY + 1);
    assert_eq!(full_prefix[0], *layers.last().unwrap());
    assert_eq!(*full_prefix.last().unwrap(), initialized.genesis_layer_id);

    let pinned_prefix = collect_layers(&store, pinned_layer_id);
    assert_eq!(pinned_prefix.len(), 401);
    assert_eq!(pinned_prefix[0], pinned_layer_id);
    assert!(!pinned_prefix.contains(layers.last().unwrap()));

    let layer_suffix =
        collect_layer_ancestry(&store, *layers.last().unwrap(), Some(pinned_layer_id));
    assert_eq!(layer_suffix.len(), HISTORY - 400);
    assert_eq!(layer_suffix[0], *layers.last().unwrap());
    assert!(!layer_suffix.contains(&pinned_layer_id));

    let full_history = collect_history(&store, branch_id, commits.last().unwrap().id, false);
    assert_eq!(full_history.len(), HISTORY);
    assert_eq!(full_history[0], commits.last().unwrap().id);
    assert_eq!(*full_history.last().unwrap(), commits[0].id);

    let pinned_history = collect_history(&store, branch_id, pinned_commit_id, false);
    assert_eq!(pinned_history.len(), 400);
    assert_eq!(pinned_history[0], pinned_commit_id);
    assert!(!pinned_history.contains(&commits.last().unwrap().id));

    let commit_suffix =
        collect_commit_ancestry(&store, commits.last().unwrap().id, Some(pinned_commit_id));
    assert_eq!(commit_suffix.len(), HISTORY - 400);
    assert_eq!(commit_suffix[0], commits.last().unwrap().id);
    assert!(!commit_suffix.contains(&pinned_commit_id));

    let owned = collect_history(&store, branch_id, commits.last().unwrap().id, true);
    assert_eq!(owned, full_history);

    drop(store);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn inherited_history_and_owned_suffix_use_distinct_stop_conditions() {
    let path = temp("inherited-history");
    let store = LayerStackStore::create(&path).unwrap();
    let initialized = initialize(&store, "api-server");
    let root = admit_leaf(&store, 50_000);
    let parent_branch_id = BranchId::new();
    let mut parent = None;
    let mut parent_commits = Vec::new();
    for serial in 0..6_u64 {
        let root_id = if serial == 0 {
            root
        } else {
            admit_leaf(&store, 50_000 + serial)
        };
        let commit = CommitRecord {
            id: CommitId::derive(root_id, parent, initialized.genesis_layer_id),
            root_id,
            parent_commit_id: parent,
            base_layer_id: initialized.genesis_layer_id,
        };
        admit_commit(&store, commit);
        let branch = BranchRecord {
            id: parent_branch_id,
            layer_stack_id: initialized.layer_stack_id,
            name: EntityName::new("main").unwrap(),
            base_layer_id: initialized.genesis_layer_id,
            head_commit_id: Some(commit.id),
            forked_from_layer_id: Some(initialized.genesis_layer_id),
            forked_from_branch_id: None,
            forked_from_commit_id: None,
        };
        LayerStackEndpoint::publish_branch(&store, &branch, parent).unwrap();
        parent = Some(commit.id);
        parent_commits.push(commit);
    }

    let origin = parent_commits[3];
    let child_branch_id = BranchId::new();
    let empty_child = BranchRecord {
        id: child_branch_id,
        layer_stack_id: initialized.layer_stack_id,
        name: EntityName::new("search-rollout").unwrap(),
        base_layer_id: origin.base_layer_id,
        head_commit_id: Some(origin.id),
        forked_from_layer_id: None,
        forked_from_branch_id: Some(parent_branch_id),
        forked_from_commit_id: Some(origin.id),
    };
    assert_eq!(
        LayerStackEndpoint::publish_branch(&store, &empty_child, None).unwrap(),
        PushResult::NoChanges
    );
    assert!(store.branch(child_branch_id).unwrap().is_none());

    let mut child_parent = Some(origin.id);
    let mut child_commits = Vec::new();
    for serial in 0..3_u64 {
        let root_id = admit_leaf(&store, 60_000 + serial);
        let commit = CommitRecord {
            id: CommitId::derive(root_id, child_parent, initialized.genesis_layer_id),
            root_id,
            parent_commit_id: child_parent,
            base_layer_id: initialized.genesis_layer_id,
        };
        admit_commit(&store, commit);
        let branch = BranchRecord {
            id: child_branch_id,
            layer_stack_id: initialized.layer_stack_id,
            name: EntityName::new("search-rollout").unwrap(),
            base_layer_id: initialized.genesis_layer_id,
            head_commit_id: Some(commit.id),
            forked_from_layer_id: None,
            forked_from_branch_id: Some(parent_branch_id),
            forked_from_commit_id: Some(origin.id),
        };
        let observed = child_commits.last().map(|known: &CommitRecord| known.id);
        LayerStackEndpoint::publish_branch(&store, &branch, observed).unwrap();
        child_parent = Some(commit.id);
        child_commits.push(commit);
    }

    let child_head = child_commits.last().unwrap().id;
    let full = collect_history(&store, child_branch_id, child_head, false);
    let owned = collect_history(&store, child_branch_id, child_head, true);
    assert_eq!(full.len(), 4 + child_commits.len());
    assert_eq!(owned.len(), child_commits.len());
    assert_eq!(owned[0], child_head);
    assert_eq!(*owned.last().unwrap(), child_commits[0].id);
    assert!(!owned.contains(&origin.id));
    assert_eq!(full[child_commits.len()], origin.id);
    assert_eq!(
        store
            .branch_fact(child_branch_id)
            .unwrap()
            .unwrap()
            .forked_from_commit_id,
        Some(origin.id)
    );

    drop(store);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn project_scoped_names_deduplicate_objects_and_reject_cross_stack_push() {
    let path = temp("project-isolation");
    let store = LayerStackStore::create(&path).unwrap();
    let api = initialize(&store, "api-server");
    let web = initialize(&store, "web-client");
    let shared_root = admit_leaf(&store, 70_000);

    let api_commit = CommitRecord {
        id: CommitId::derive(shared_root, None, api.genesis_layer_id),
        root_id: shared_root,
        parent_commit_id: None,
        base_layer_id: api.genesis_layer_id,
    };
    admit_commit(&store, api_commit);
    let api_branch_id = BranchId::new();
    let api_branch = BranchRecord {
        id: api_branch_id,
        layer_stack_id: api.layer_stack_id,
        name: EntityName::new("main").unwrap(),
        base_layer_id: api.genesis_layer_id,
        head_commit_id: Some(api_commit.id),
        forked_from_layer_id: Some(api.genesis_layer_id),
        forked_from_branch_id: None,
        forked_from_commit_id: None,
    };
    LayerStackEndpoint::publish_branch(&store, &api_branch, None).unwrap();

    let inventory_before = store.inventory_page(None, 512).unwrap().entries.len();
    let web_commit = CommitRecord {
        id: CommitId::derive(shared_root, None, web.genesis_layer_id),
        root_id: shared_root,
        parent_commit_id: None,
        base_layer_id: web.genesis_layer_id,
    };
    admit_commit(&store, web_commit);
    let web_branch = BranchRecord {
        id: BranchId::new(),
        layer_stack_id: web.layer_stack_id,
        name: EntityName::new("main").unwrap(),
        base_layer_id: web.genesis_layer_id,
        head_commit_id: Some(web_commit.id),
        forked_from_layer_id: Some(web.genesis_layer_id),
        forked_from_branch_id: None,
        forked_from_commit_id: None,
    };
    LayerStackEndpoint::publish_branch(&store, &web_branch, None).unwrap();
    assert_eq!(
        store.inventory_page(None, 512).unwrap().entries.len(),
        inventory_before
    );

    let conflicting = BranchRecord {
        id: BranchId::new(),
        head_commit_id: Some(api_commit.id),
        ..api_branch.clone()
    };
    assert!(matches!(
        LayerStackEndpoint::publish_branch(&store, &conflicting, None),
        Err(StorageError::BranchNameConflict {
            layer_stack_id,
            name,
            existing_id,
            incoming_id,
        }) if layer_stack_id == api.layer_stack_id
            && name.as_str() == "main"
            && existing_id == api_branch_id
            && incoming_id == conflicting.id
    ));

    let cross_stack = BranchRecord {
        id: BranchId::new(),
        layer_stack_id: web.layer_stack_id,
        name: EntityName::new("invalid-cross-stack").unwrap(),
        base_layer_id: api.genesis_layer_id,
        head_commit_id: Some(api_commit.id),
        forked_from_layer_id: Some(web.genesis_layer_id),
        forked_from_branch_id: None,
        forked_from_commit_id: None,
    };
    assert!(matches!(
        LayerStackEndpoint::publish_branch(&store, &cross_stack, None),
        Err(StorageError::Integrity("pushed Branch ownership"))
    ));

    let cross_add_id = BranchId::new();
    let cross_add = BranchRecord {
        id: cross_add_id,
        layer_stack_id: api.layer_stack_id,
        name: EntityName::new("cross-add").unwrap(),
        base_layer_id: api.genesis_layer_id,
        head_commit_id: Some(api_commit.id),
        forked_from_layer_id: Some(api.genesis_layer_id),
        forked_from_branch_id: None,
        forked_from_commit_id: None,
    };
    LayerStackEndpoint::publish_branch(&store, &cross_add, None).unwrap();

    let first_stack_page = store.layer_stack_record_page(None, 1).unwrap();
    assert_eq!(first_stack_page.records.len(), 1);
    let second_stack_page = store
        .layer_stack_record_page(first_stack_page.continuation, 1)
        .unwrap();
    assert_eq!(second_stack_page.records.len(), 1);
    let stack_names = first_stack_page
        .records
        .iter()
        .chain(&second_stack_page.records)
        .map(|record| record.name.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        stack_names,
        std::collections::BTreeSet::from(["api-server", "web-client"])
    );

    let first_api_page = store
        .branch_record_page(Some(api.layer_stack_id), None, 1)
        .unwrap();
    assert_eq!(first_api_page.records.len(), 1);
    let second_api_page = store
        .branch_record_page(Some(api.layer_stack_id), first_api_page.continuation, 1)
        .unwrap();
    assert_eq!(second_api_page.records.len(), 1);
    assert!(second_api_page.continuation.is_none());
    assert!(first_api_page
        .records
        .iter()
        .chain(&second_api_page.records)
        .all(|record| record.layer_stack_id == api.layer_stack_id));

    rusqlite::Connection::open(&path)
        .unwrap()
        .execute(
            "UPDATE branches SET layer_stack_id=?1 WHERE branch_id=?2",
            rusqlite::params![
                layerfs_storage::StorageId::as_slice(&web.layer_stack_id),
                layerfs_storage::StorageId::as_slice(&cross_add_id),
            ],
        )
        .unwrap();
    assert!(matches!(
        store.add_layer(cross_add_id),
        Err(StorageError::Integrity("Branch LayerStack ownership"))
    ));

    drop(store);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn an_advance_verifies_only_the_new_owned_suffix() {
    let path = temp("suffix-only");
    let store = LayerStackStore::create(&path).unwrap();
    let initialized = initialize(&store, "api-server");
    let branch_id = BranchId::new();
    let first_root = admit_leaf(&store, 80_000);
    let first = CommitRecord {
        id: CommitId::derive(first_root, None, initialized.genesis_layer_id),
        root_id: first_root,
        parent_commit_id: None,
        base_layer_id: initialized.genesis_layer_id,
    };
    admit_commit(&store, first);
    let mut branch = BranchRecord {
        id: branch_id,
        layer_stack_id: initialized.layer_stack_id,
        name: EntityName::new("main").unwrap(),
        base_layer_id: initialized.genesis_layer_id,
        head_commit_id: Some(first.id),
        forked_from_layer_id: Some(initialized.genesis_layer_id),
        forked_from_branch_id: None,
        forked_from_commit_id: None,
    };
    LayerStackEndpoint::publish_branch(&store, &branch, None).unwrap();

    rusqlite::Connection::open(&path)
        .unwrap()
        .execute(
            "UPDATE objects SET bytes=x'00' WHERE object_id=?1",
            [first_root.as_bytes().as_slice()],
        )
        .unwrap();

    let second_root = admit_leaf(&store, 80_001);
    let second = CommitRecord {
        id: CommitId::derive(second_root, Some(first.id), initialized.genesis_layer_id),
        root_id: second_root,
        parent_commit_id: Some(first.id),
        base_layer_id: initialized.genesis_layer_id,
    };
    admit_commit(&store, second);
    branch.head_commit_id = Some(second.id);
    assert_eq!(
        LayerStackEndpoint::publish_branch(&store, &branch, Some(first.id)).unwrap(),
        PushResult::Advanced {
            previous: first.id,
            commit_id: second.id,
        }
    );

    drop(store);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn authority_point_and_name_queries_are_indexed() {
    let path = temp("query-plans");
    let store = LayerStackStore::create(&path).unwrap();
    initialize(&store, "api-server");
    let connection = rusqlite::Connection::open(&path).unwrap();
    for sql in [
        "SELECT layer_stack_id FROM layer_stacks WHERE name=?1",
        "SELECT layer_id FROM layers WHERE layer_id=?1",
        "SELECT commit_id FROM commits WHERE commit_id=?1",
        "SELECT branch_id FROM branches WHERE layer_stack_id=?1 AND name=?2",
    ] {
        let explain = format!("EXPLAIN QUERY PLAN {sql}");
        let mut statement = connection.prepare(&explain).unwrap();
        let parameters =
            std::iter::repeat_n(rusqlite::types::Value::Null, statement.parameter_count());
        let details = statement
            .query_map(rusqlite::params_from_iter(parameters), |row| {
                row.get::<_, String>(3)
            })
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap();
        assert!(
            details.iter().any(|detail| detail.contains("SEARCH")),
            "{sql}: {details:?}"
        );
        assert!(
            details.iter().all(|detail| !detail.starts_with("SCAN ")),
            "{sql}: {details:?}"
        );
    }
    let layer_page = explain(
        &connection,
        "SELECT s.layer_stack_id,s.name,s.head_layer_id FROM layer_stacks s
         WHERE s.layer_stack_id>?1
         ORDER BY s.layer_stack_id LIMIT ?2",
        vec![
            rusqlite::types::Value::Blob(vec![0_u8; 17]),
            rusqlite::types::Value::Integer(513),
        ],
    );
    assert!(
        layer_page.iter().any(|detail| detail.contains("SEARCH")),
        "LayerStack page: {layer_page:?}"
    );
    let branch_page = explain(
        &connection,
        "SELECT b.branch_id,b.layer_stack_id,b.name,b.base_layer_id,b.head_commit_id,
                b.forked_from_layer_id,b.forked_from_branch_id,b.forked_from_commit_id
         FROM branches b
         WHERE b.layer_stack_id=?1 AND b.branch_id>?2
         ORDER BY b.branch_id LIMIT ?3",
        vec![
            rusqlite::types::Value::Blob(vec![0_u8; 17]),
            rusqlite::types::Value::Blob(vec![0_u8; 17]),
            rusqlite::types::Value::Integer(513),
        ],
    );
    assert!(
        branch_page
            .iter()
            .any(|detail| detail.contains("branch_identity") && detail.contains("SEARCH")),
        "Branch page: {branch_page:?}"
    );
    drop(connection);
    drop(store);
    std::fs::remove_file(path).unwrap();
}

fn explain(
    connection: &rusqlite::Connection,
    sql: &str,
    parameters: Vec<rusqlite::types::Value>,
) -> Vec<String> {
    let mut statement = connection
        .prepare(&format!("EXPLAIN QUERY PLAN {sql}"))
        .unwrap();
    statement
        .query_map(rusqlite::params_from_iter(parameters), |row| {
            row.get::<_, String>(3)
        })
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap()
}

fn initialize(store: &LayerStackStore, name: &str) -> InitializeLayerStackResult {
    store
        .initialize_layerstack(
            EntityName::new(name).unwrap(),
            LayerStackInitialization::Empty,
        )
        .unwrap()
}

fn admit_leaf(store: &LayerStackStore, serial: u64) -> layerfs_content::ObjectId {
    let canonical = layerfs_content::encode_bytes_object(&serial.to_be_bytes()).unwrap();
    let object = CanonicalObject::new(canonical).unwrap();
    let id = object.id;
    LayerStackEndpoint::admit_objects(store, &[object]).unwrap();
    id
}

fn admit_commit(store: &LayerStackStore, commit: CommitRecord) {
    LayerStackEndpoint::admit_facts(store, &[Fact::Commit(commit)]).unwrap();
}

fn collect_layers(store: &LayerStackStore, through: LayerId) -> Vec<LayerId> {
    let mut cursor = None;
    let mut records = Vec::new();
    loop {
        let LayerPrefixPage {
            records: page,
            continuation,
        } = store.layer_prefix_page(through, cursor, 128).unwrap();
        assert!(!page.is_empty());
        assert!(page.len() <= 128);
        records.extend(page.into_iter().map(|record| record.id));
        let Some(next) = continuation else { break };
        cursor = Some(next);
    }
    records
}

fn collect_layer_ancestry(
    store: &LayerStackStore,
    through: LayerId,
    stop_exclusive: Option<LayerId>,
) -> Vec<LayerId> {
    let mut cursor = None;
    let mut records = Vec::new();
    loop {
        let page = store
            .layer_ancestry_page(through, stop_exclusive, cursor, 128)
            .unwrap();
        assert!(page.records.len() <= 128);
        records.extend(page.records.into_iter().map(|record| record.id));
        let Some(next) = page.continuation else { break };
        cursor = Some(next);
    }
    records
}

fn collect_history(
    store: &LayerStackStore,
    branch_id: BranchId,
    through: CommitId,
    owned: bool,
) -> Vec<CommitId> {
    let mut cursor = None;
    let mut records = Vec::new();
    loop {
        let page = if owned {
            store.owned_commit_page(branch_id, through, cursor, 128)
        } else {
            store.commit_history_page(branch_id, through, cursor, 128)
        }
        .unwrap();
        assert!(!page.records.is_empty());
        assert!(page.records.len() <= 128);
        records.extend(page.records.into_iter().map(|record| record.id));
        let Some(next) = page.continuation else { break };
        cursor = Some(next);
    }
    records
}

fn collect_commit_ancestry(
    store: &LayerStackStore,
    through: CommitId,
    stop_exclusive: Option<CommitId>,
) -> Vec<CommitId> {
    let mut cursor = None;
    let mut records = Vec::new();
    loop {
        let page = store
            .commit_ancestry_page(through, stop_exclusive, cursor, 128)
            .unwrap();
        assert!(page.records.len() <= 128);
        records.extend(page.records.into_iter().map(|record| record.id));
        let Some(next) = page.continuation else { break };
        cursor = Some(next);
    }
    records
}

fn temp(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "layerfs-v2-authority-{name}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
}
