use layerfs_content::filesystem::ContentChange;
use layerfs_layerstack_store::{
    apply_changes, CommitOutcome, EntityName, LayerStackInitialization, LayerStackStore,
    LocalForkSource, StoreError,
};

#[test]
fn v4_migration_and_v5_staging_preserve_exact_publication_semantics() {
    let root = temp("migration-staging");
    let path = root.join("store.sqlite");
    let store = LayerStackStore::create(&path).unwrap();
    let initialized = store
        .initialize_layerstack(
            EntityName::new("demo").unwrap(),
            LayerStackInitialization::Empty,
        )
        .unwrap();
    let branch_id = store
        .fork_branch(
            EntityName::new("main").unwrap(),
            LocalForkSource::Layer {
                layer_id: initialized.genesis_layer_id,
            },
        )
        .unwrap();
    let original = store.pin_branch(branch_id).unwrap();
    let original_root = original.root;
    let original_branch = original.branch.clone();
    drop(original);
    drop(store);

    let connection = rusqlite::Connection::open(&path).unwrap();
    connection
        .execute_batch("DROP TABLE workspace_stages; PRAGMA user_version=4;")
        .unwrap();
    drop(connection);
    let malformed = root.join("malformed.sqlite");
    std::fs::copy(&path, &malformed).unwrap();
    let connection = rusqlite::Connection::open(&malformed).unwrap();
    connection
        .execute_batch("CREATE TABLE unexpected(value INTEGER) STRICT;")
        .unwrap();
    drop(connection);
    assert!(matches!(
        LayerStackStore::connect(&malformed),
        Err(StoreError::WrongStoreSchema)
    ));
    let connection = rusqlite::Connection::open(&malformed).unwrap();
    assert_eq!(pragma(&connection, "user_version"), 4);
    assert_eq!(
        connection
            .query_row(
                "SELECT count(*) FROM sqlite_schema WHERE name='unexpected'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .unwrap(),
        1
    );
    drop(connection);

    let store = LayerStackStore::connect(&path).unwrap();
    assert_eq!(store.pin_branch(branch_id).unwrap().root, original_root);
    assert_eq!(store.branch(branch_id).unwrap(), Some(original_branch));

    let pinned = store.pin_branch(branch_id).unwrap();
    let candidate = write_candidate(&pinned.reader, pinned.root, "first", [1; 32]);
    let candidate_statements = candidate.objects.len();
    let retained_workspace = [1; 16];
    layerfs_layerstack_store::set_transaction_failure_at(Some(candidate_statements + 3));
    let failed = store.commit_workspace_candidate(
        retained_workspace,
        &pinned.branch,
        pinned.root,
        pinned.branch.base_layer_id,
        candidate,
    );
    layerfs_layerstack_store::set_transaction_failure_at(None);
    assert!(matches!(
        failed,
        Err(StoreError::Integrity("injected transaction failure"))
    ));
    let retained = store.workspace_stage(retained_workspace).unwrap().unwrap();
    assert_eq!(retained.branch_id, branch_id);
    assert_eq!(store.pin_branch(branch_id).unwrap().root, pinned.root);
    assert_eq!(store.store_counts().unwrap().commits, 0);

    let candidate = write_candidate(&pinned.reader, pinned.root, "first", [1; 32]);
    let committed = store
        .commit_workspace_candidate(
            retained_workspace,
            &pinned.branch,
            pinned.root,
            pinned.branch.base_layer_id,
            candidate,
        )
        .unwrap();
    let CommitOutcome::Committed {
        commit_id,
        root_id: committed_root,
        ..
    } = committed
    else {
        panic!("expected committed outcome")
    };
    assert!(store.workspace_stage(retained_workspace).unwrap().is_none());
    assert_eq!(
        store.branch(branch_id).unwrap().unwrap().head_commit_id,
        Some(commit_id)
    );
    assert_eq!(store.pin_branch(branch_id).unwrap().root, committed_root);

    let current = store.pin_branch(branch_id).unwrap();
    let no_op = apply_changes(&current.reader, current.root, &[], [2; 32]).unwrap();
    assert_eq!(
        store
            .commit_workspace_candidate(
                [2; 16],
                &current.branch,
                current.root,
                current.branch.base_layer_id,
                no_op,
            )
            .unwrap(),
        CommitOutcome::UpToDate {
            root_id: current.root
        }
    );
    assert!(store.workspace_stage([2; 16]).unwrap().is_none());
    assert_eq!(store.store_counts().unwrap().commits, 1);

    let stale = current;
    let winner = write_candidate(&stale.reader, stale.root, "winner", [3; 32]);
    store
        .commit_workspace_candidate(
            [3; 16],
            &stale.branch,
            stale.root,
            stale.branch.base_layer_id,
            winner,
        )
        .unwrap();
    let winner_root = store.pin_branch(branch_id).unwrap().root;

    let stale_candidate = write_candidate(&stale.reader, stale.root, "stale", [4; 32]);
    let stale_workspace = [4; 16];
    assert!(matches!(
        store.commit_workspace_candidate(
            stale_workspace,
            &stale.branch,
            stale.root,
            stale.branch.base_layer_id,
            stale_candidate,
        ),
        Err(StoreError::CommitHeadMoved { .. })
    ));
    let retained = store.workspace_stage(stale_workspace).unwrap().unwrap();
    assert_eq!(store.pin_branch(branch_id).unwrap().root, winner_root);

    let replacement = write_candidate(&stale.reader, stale.root, "replacement", [5; 32]);
    assert!(matches!(
        store.commit_workspace_candidate(
            stale_workspace,
            &stale.branch,
            stale.root,
            stale.branch.base_layer_id,
            replacement,
        ),
        Err(StoreError::InvalidInput("Workspace stage already retained"))
    ));
    assert_eq!(
        store.workspace_stage(stale_workspace).unwrap(),
        Some(retained)
    );
    assert!(store.discard_workspace_stage(stale_workspace).unwrap());
    assert!(!store.discard_workspace_stage(stale_workspace).unwrap());

    drop(pinned);
    drop(stale);
    drop(store);
    let connection = rusqlite::Connection::open(&path).unwrap();
    assert_eq!(pragma(&connection, "application_id"), 0x4c46_534c);
    assert_eq!(pragma(&connection, "user_version"), 5);
    let columns = connection
        .prepare("SELECT name FROM pragma_table_info('workspace_stages') ORDER BY cid")
        .unwrap()
        .query_map([], |row| row.get::<_, String>(0))
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap();
    assert_eq!(columns, ["workspace_id", "branch_id", "root_id"]);
    let extra_schema = connection
        .query_row(
            "SELECT count(*) FROM sqlite_schema WHERE name LIKE 'workspace_%'",
            [],
            |row| row.get::<_, i64>(0),
        )
        .unwrap();
    assert_eq!(extra_schema, 1);
    drop(connection);
    std::fs::remove_dir_all(root).unwrap();
}

fn write_candidate(
    reader: &impl layerfs_layerstack_store::ObjectSource,
    root: layerfs_content::ObjectId,
    bytes: &str,
    seed: [u8; 32],
) -> layerfs_layerstack_store::BuiltRoot {
    apply_changes(
        reader,
        root,
        &[ContentChange::Write {
            path: "file".to_owned(),
            bytes: bytes.as_bytes().to_vec(),
            mode: 0o644,
        }],
        seed,
    )
    .unwrap()
}

fn pragma(connection: &rusqlite::Connection, name: &str) -> i64 {
    connection
        .pragma_query_value(None, name, |row| row.get(0))
        .unwrap()
}

fn temp(label: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!(
        "layerfs-v5-{label}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&path).unwrap();
    path
}
