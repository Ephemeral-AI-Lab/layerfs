use layerfs_content::filesystem::ContentChange;
use layerfs_layerstack_store::{
    apply_changes, AddLayerResult, CommitOutcome, EntityName, LayerStackInitialization,
    LayerStackStore, LocalForkSource, ObjectSource, StoreError,
};
use std::collections::BTreeSet;

#[test]
fn exact_v4_schema_runtime_and_old_schema_rejection() {
    let root = temp("schema");
    let path = root.join("store.sqlite");
    let store = LayerStackStore::create(&path).unwrap();
    assert_eq!(store_files(&root), vec!["store.sqlite"]);
    drop(store);
    let connection =
        rusqlite::Connection::open_with_flags(&path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .unwrap();
    assert_eq!(pragma(&connection, "application_id"), 0x4c46_534c);
    assert_eq!(pragma(&connection, "user_version"), 4);
    assert_eq!(pragma(&connection, "page_size"), 65_536);

    let tables = connection
        .prepare(
            "SELECT name,ncol,wr,strict FROM pragma_table_list \
             WHERE schema='main' AND name NOT LIKE 'sqlite_%' ORDER BY name",
        )
        .unwrap()
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, i64>(3)?,
            ))
        })
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap();
    assert_eq!(
        tables,
        vec![
            ("branches".to_owned(), 5, 1, 1),
            ("commits".to_owned(), 4, 1, 1),
            ("layer_stacks".to_owned(), 3, 1, 1),
            ("layers".to_owned(), 6, 1, 1),
            ("objects".to_owned(), 2, 0, 1),
        ]
    );
    assert_eq!(tables.iter().map(|table| table.1).sum::<i64>(), 20);
    let indexes = connection
        .prepare(
            "SELECT name FROM sqlite_schema \
             WHERE type='index' AND name NOT LIKE 'sqlite_%' ORDER BY name",
        )
        .unwrap()
        .query_map([], |row| row.get::<_, String>(0))
        .unwrap()
        .collect::<rusqlite::Result<BTreeSet<_>>>()
        .unwrap();
    assert_eq!(
        indexes,
        BTreeSet::from([
            "branch_identity".to_owned(),
            "branch_names".to_owned(),
            "layer_identity".to_owned(),
            "layer_stack_names".to_owned(),
            "layers_child".to_owned(),
            "layers_genesis".to_owned(),
            "layers_source".to_owned(),
        ])
    );
    drop(connection);

    let old = root.join("old.sqlite");
    let connection = rusqlite::Connection::open(&old).unwrap();
    connection
        .execute_batch(
            "PRAGMA application_id=1279677260; PRAGMA user_version=3; \
             CREATE TABLE store(singleton INTEGER PRIMARY KEY) STRICT;",
        )
        .unwrap();
    drop(connection);
    assert!(matches!(
        LayerStackStore::connect(&old),
        Err(StoreError::WrongStoreSchema)
    ));
    let connection = rusqlite::Connection::open(&old).unwrap();
    assert_eq!(pragma(&connection, "user_version"), 3);
    assert_eq!(
        connection
            .query_row(
                "SELECT count(*) FROM sqlite_schema WHERE name='store'",
                [],
                |row| { row.get::<_, i64>(0) }
            )
            .unwrap(),
        1
    );

    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn one_store_initialize_fork_commit_add_and_dedup_are_atomic() {
    let root = temp("lifecycle");
    let store = LayerStackStore::create(root.join("store.sqlite")).unwrap();
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
    let pinned = store.pin_branch(branch_id).unwrap();
    let built = apply_changes(
        &pinned.reader,
        pinned.root,
        &[ContentChange::Write {
            path: "hello".to_owned(),
            bytes: b"world".to_vec(),
            mode: 0o644,
        }],
        [7; 32],
    )
    .unwrap();
    let commit_id = match store
        .commit_candidate(
            &pinned.branch,
            pinned.root,
            pinned.branch.base_layer_id,
            built,
        )
        .unwrap()
    {
        CommitOutcome::Committed {
            commit_id,
            candidate_objects,
            inserted_objects,
            reused_objects,
            candidate_bytes,
            inserted_bytes,
            reused_bytes,
            ..
        } => {
            assert_eq!(candidate_objects, inserted_objects + reused_objects);
            assert_eq!(candidate_bytes, inserted_bytes + reused_bytes);
            commit_id
        }
        outcome => panic!("unexpected Commit outcome: {outcome:?}"),
    };
    assert_eq!(
        store.branch(branch_id).unwrap().unwrap().head_commit_id,
        Some(commit_id)
    );
    assert!(store.commit(commit_id).unwrap().is_some());
    let layer_id = match store.add_layer(branch_id).unwrap() {
        AddLayerResult::Added { layer_id } => layer_id,
        outcome => panic!("unexpected Add outcome: {outcome:?}"),
    };
    assert_eq!(
        store.add_layer(branch_id).unwrap(),
        AddLayerResult::UpToDate { layer_id }
    );
    let counts = store.store_counts().unwrap();
    assert_eq!(counts.layer_stacks, 1);
    assert_eq!(counts.layers, 2);
    assert_eq!(counts.branches, 1);
    assert_eq!(counts.commits, 1);
    assert_eq!(store.canonical_storage().unwrap().objects, counts.objects);

    drop(store);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn entity_names_enforce_the_exact_boundary() {
    assert!(EntityName::new("a").is_ok());
    assert!(EntityName::new("a".repeat(63)).is_ok());
    for invalid in ["", "A", "-a", "a-", "a/b", "a b", "é"] {
        assert!(EntityName::new(invalid).is_err(), "accepted {invalid:?}");
    }
    assert!(EntityName::new("a".repeat(64)).is_err());
}

#[test]
fn no_op_commit_writes_nothing_and_every_publication_statement_rolls_back() {
    let root = temp("commit-atomicity");
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
    let pinned = store.pin_branch(branch_id).unwrap();
    let before_counts = store.store_counts().unwrap();
    let before_version = store.data_version().unwrap();
    let before_bytes = std::fs::metadata(&path).unwrap().len();
    let unchanged = apply_changes(&pinned.reader, pinned.root, &[], [1; 32]).unwrap();
    assert_eq!(
        store
            .commit_candidate(
                &pinned.branch,
                pinned.root,
                pinned.branch.base_layer_id,
                unchanged,
            )
            .unwrap(),
        CommitOutcome::UpToDate {
            root_id: pinned.root
        }
    );
    assert_eq!(store.store_counts().unwrap(), before_counts);
    assert_eq!(store.data_version().unwrap(), before_version);
    assert_eq!(std::fs::metadata(&path).unwrap().len(), before_bytes);
    assert_eq!(store_files(&root), vec!["store.sqlite"]);

    let candidate = apply_changes(
        &pinned.reader,
        pinned.root,
        &[ContentChange::Write {
            path: "hello".to_owned(),
            bytes: b"world".to_vec(),
            mode: 0o644,
        }],
        [2; 32],
    )
    .unwrap();
    let missing = candidate
        .objects
        .ids_in_order(usize::MAX)
        .unwrap()
        .unwrap()
        .iter()
        .filter(|id| store.read_object(**id).is_err())
        .count() as u64;
    for statement in 1..=missing + 2 {
        let candidate = apply_changes(
            &pinned.reader,
            pinned.root,
            &[ContentChange::Write {
                path: "hello".to_owned(),
                bytes: b"world".to_vec(),
                mode: 0o644,
            }],
            [2; 32],
        )
        .unwrap();
        layerfs_layerstack_store::set_transaction_failure_at(Some(statement));
        let result = store.commit_candidate(
            &pinned.branch,
            pinned.root,
            pinned.branch.base_layer_id,
            candidate,
        );
        layerfs_layerstack_store::set_transaction_failure_at(None);
        assert!(matches!(
            result,
            Err(StoreError::Integrity("injected transaction failure"))
        ));
        assert_eq!(store.store_counts().unwrap(), before_counts);
        assert_eq!(store.branch(branch_id).unwrap().unwrap(), pinned.branch);
    }

    drop(store);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn concurrent_commit_has_one_cas_winner() {
    let root = temp("concurrent-commit");
    let store = std::sync::Arc::new(LayerStackStore::create(root.join("store.sqlite")).unwrap());
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
    let pinned = store.pin_branch(branch_id).unwrap();
    let pinned_root = pinned.root;
    let candidates = [b"one".as_slice(), b"two".as_slice()].map(|bytes| {
        apply_changes(
            &pinned.reader,
            pinned_root,
            &[ContentChange::Write {
                path: "winner".to_owned(),
                bytes: bytes.to_vec(),
                mode: 0o644,
            }],
            [3; 32],
        )
        .unwrap()
    });
    let threads = candidates.map(|candidate| {
        let store = store.clone();
        let branch = pinned.branch.clone();
        std::thread::spawn(move || {
            store.commit_candidate(&branch, pinned_root, branch.base_layer_id, candidate)
        })
    });
    let outcomes = threads.map(|thread| thread.join().unwrap());
    assert_eq!(
        outcomes
            .iter()
            .filter(|outcome| matches!(outcome, Ok(CommitOutcome::Committed { .. })))
            .count(),
        1
    );
    assert_eq!(
        outcomes
            .iter()
            .filter(|outcome| matches!(outcome, Err(StoreError::CommitHeadMoved { .. })))
            .count(),
        1
    );
    assert_eq!(store.store_counts().unwrap().commits, 1);

    drop(store);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn visible_missing_and_same_length_corrupt_objects_are_integrity_errors() {
    let root = temp("object-integrity");
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
    let pinned = store.pin_branch(branch_id).unwrap();
    let visible_root = pinned.root;
    drop(pinned);
    drop(store);
    let connection = rusqlite::Connection::open(&path).unwrap();
    connection
        .execute(
            "UPDATE objects SET bytes=zeroblob(length(bytes)) WHERE object_id=?1",
            [visible_root.as_bytes().as_slice()],
        )
        .unwrap();
    drop(connection);
    let store = LayerStackStore::connect(&path).unwrap();
    assert!(matches!(
        store
            .snapshot_reader(visible_root)
            .read_object(visible_root),
        Err(StoreError::Integrity(_))
    ));
    drop(store);
    let connection = rusqlite::Connection::open(&path).unwrap();
    connection
        .execute(
            "DELETE FROM objects WHERE object_id=?1",
            [visible_root.as_bytes().as_slice()],
        )
        .unwrap();
    drop(connection);
    assert!(matches!(
        LayerStackStore::connect(&path),
        Err(StoreError::Integrity("foreign key check"))
    ));

    std::fs::remove_dir_all(root).unwrap();
}

fn pragma(connection: &rusqlite::Connection, name: &str) -> i64 {
    connection
        .pragma_query_value(None, name, |row| row.get(0))
        .unwrap()
}

fn temp(label: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!(
        "layerfs-v4-{label}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&path).unwrap();
    path
}

fn store_files(root: &std::path::Path) -> Vec<String> {
    let mut files = std::fs::read_dir(root)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .collect::<Vec<_>>();
    files.sort();
    files
}
