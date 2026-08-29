use layerfs_branch_store::BranchStore;
use layerfs_layer_store::LayerStore;
use layerfs_stack_store::StackStore;
use layerfs_storage_core::{
    dependency_order, AddLayerSource, BranchId, Change, CommitId, FactKind, ObjectSource,
    RefOutcome, StorageId, StoreEndpoint,
};
use std::collections::BTreeMap;
use std::sync::{Arc, Barrier};

const INSTALLATIONS: usize = 10;

#[test]
fn ten_direct_installs_share_one_payload_set_and_serialize_adds() {
    let run = run_dir("direct");
    let layer_path = run.join("layer.sqlite");
    let branch_path = run.join("branch.sqlite");
    let layer = Arc::new(LayerStore::open(&layer_path).unwrap());
    let (history, genesis) = layer.provision().unwrap();
    let genesis_objects = objects(&layer_path);
    let branch = BranchStore::open(&branch_path, layer.clone()).unwrap();
    let package = package_changes();
    let installs = create_installs(&branch, history.id, genesis.id, None, &package);
    let expected = union(
        &genesis_objects,
        &reachable(&branch, branch.root(installs[0].0).unwrap()),
    );
    let private_objects = objects(&branch_path);
    for (id, _) in &installs {
        branch.push_branch(*id).unwrap();
    }
    let barrier = Arc::new(Barrier::new(installs.len()));
    let threads = installs
        .iter()
        .map(|(branch_id, commit_id)| {
            let layer = layer.clone();
            let barrier = barrier.clone();
            let (branch_id, commit_id) = (*branch_id, *commit_id);
            std::thread::spawn(move || {
                barrier.wait();
                layer
                    .add_layer(
                        history.id,
                        AddLayerSource::BranchSource {
                            branch_id,
                            commit_id,
                        },
                    )
                    .unwrap()
                    .result_id
            })
        })
        .collect::<Vec<_>>();
    let results = threads
        .into_iter()
        .map(|thread| thread.join().unwrap())
        .collect::<Vec<_>>();
    assert!(results.windows(2).all(|pair| pair[0] == pair[1]));
    assert_eq!(objects(&branch_path), private_objects);
    drop(branch);
    drop(layer);
    assert_eq!(count(&branch_path, "branches"), 10);
    assert_eq!(count(&branch_path, "commits"), 2);
    assert_eq!(count(&layer_path, "add_results"), 10);
    assert_eq!(count(&layer_path, "layers"), 2);
    assert_eq!(objects(&layer_path), expected);
    assert_eq!(count(&layer_path, "commits"), 2);
    assert_accounting("direct", "branch", &branch_path, &private_objects, 12);
    assert_accounting("direct", "layer", &layer_path, &expected, 25);
    std::fs::remove_dir_all(run).unwrap();
}

#[test]
fn ten_stacked_installs_share_one_payload_set_and_serialize_adds() {
    let run = run_dir("stacked");
    let layer_path = run.join("layer.sqlite");
    let stack_path = run.join("stack.sqlite");
    let branch_path = run.join("branch.sqlite");
    let layer = Arc::new(LayerStore::open(&layer_path).unwrap());
    let (layer_history, genesis) = layer.provision().unwrap();
    let genesis_objects = objects(&layer_path);
    let stack = Arc::new(StackStore::open(&stack_path, layer.clone()).unwrap());
    stack
        .pull_layer_history(layer_history.id, genesis.id)
        .unwrap();
    let (stack_history, seed) = stack
        .create_stack_history_from_layer(layer_history.id, genesis.id)
        .unwrap();
    let branch = BranchStore::open(&branch_path, stack.clone()).unwrap();
    let package = package_changes();
    let installs = create_installs(
        &branch,
        layer_history.id,
        genesis.id,
        Some((stack_history.id, seed.id)),
        &package,
    );
    let expected = union(
        &genesis_objects,
        &reachable(&branch, branch.root(installs[0].0).unwrap()),
    );
    for (id, _) in &installs {
        branch.push_branch(*id).unwrap();
    }
    let private_objects = objects(&branch_path);
    let barrier = Arc::new(Barrier::new(installs.len()));
    let threads = installs
        .iter()
        .map(|(branch_id, commit_id)| {
            let stack = stack.clone();
            let barrier = barrier.clone();
            let (branch_id, commit_id) = (*branch_id, *commit_id);
            std::thread::spawn(move || {
                barrier.wait();
                stack
                    .add_stack(stack_history.id, branch_id, commit_id)
                    .unwrap()
                    .result_id
            })
        })
        .collect::<Vec<_>>();
    let results = threads
        .into_iter()
        .map(|thread| thread.join().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        results
            .iter()
            .copied()
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        10
    );
    let head = stack
        .stack_history(stack_history.id)
        .unwrap()
        .unwrap()
        .head_stack_id;
    assert!(matches!(
        stack.push_stack(head).unwrap(),
        RefOutcome::Created(_) | RefOutcome::FastForwarded(_)
    ));
    assert!(matches!(
        stack.push_stack(head).unwrap(),
        RefOutcome::UpToDate(_)
    ));
    layer
        .add_layer(layer_history.id, AddLayerSource::StackSource(head))
        .unwrap();
    assert_eq!(objects(&branch_path), private_objects);
    drop(branch);
    drop(stack);
    drop(layer);
    assert_eq!(count(&branch_path, "branches"), 10);
    assert_eq!(count(&branch_path, "commits"), 2);
    assert_eq!(count(&stack_path, "add_results"), 10);
    assert_eq!(count(&stack_path, "stacks"), 11);
    assert_eq!(objects(&stack_path), expected);
    assert_eq!(count(&stack_path, "commits"), 2);
    assert_eq!(count(&layer_path, "branches"), 10);
    assert_eq!(count(&layer_path, "layers"), 2);
    assert_eq!(objects(&layer_path), expected);
    assert_eq!(count(&layer_path, "commits"), 2);
    assert_accounting("stacked", "branch", &branch_path, &private_objects, 12);
    assert_accounting("stacked", "stack", &stack_path, &expected, 36);
    assert_accounting("stacked", "layer", &layer_path, &expected, 38);
    std::fs::remove_dir_all(run).unwrap();
}

#[test]
fn late_equal_root_add_advances_stack_and_pushes_new_branch_provenance() {
    let run = run_dir("late-equal-root");
    let layer_path = run.join("layer.sqlite");
    let stack_path = run.join("stack.sqlite");
    let branch_path = run.join("branch.sqlite");
    let layer = Arc::new(LayerStore::open(&layer_path).unwrap());
    let (layer_history, genesis) = layer.provision().unwrap();
    let stack = Arc::new(StackStore::open(&stack_path, layer.clone()).unwrap());
    stack
        .pull_layer_history(layer_history.id, genesis.id)
        .unwrap();
    let (stack_history, seed) = stack
        .create_stack_history_from_layer(layer_history.id, genesis.id)
        .unwrap();
    let branch = BranchStore::open(&branch_path, stack.clone()).unwrap();
    let small = small_install_changes();
    let installs = create_installs(
        &branch,
        layer_history.id,
        genesis.id,
        Some((stack_history.id, seed.id)),
        &small,
    );
    let (a, a_commit) = installs[0];
    let (b, b_commit) = installs[1];
    assert_eq!(a_commit, b_commit);

    branch.push_branch(a).unwrap();
    let s1 = stack
        .add_stack(stack_history.id, a, a_commit)
        .unwrap()
        .result_id;
    stack.push_stack(s1).unwrap();
    let objects_after_a = objects(&layer_path);

    branch.push_branch(b).unwrap();
    let s2 = stack
        .add_stack(stack_history.id, b, b_commit)
        .unwrap()
        .result_id;
    assert_ne!(s1, s2);
    assert_eq!(
        stack.stack(s1).unwrap().unwrap().root_id,
        stack.stack(s2).unwrap().unwrap().root_id
    );
    assert_eq!(stack.stack(s2).unwrap().unwrap().parent_id, Some(s1));
    let rows_before_repeat = (
        count(&stack_path, "stacks"),
        count(&stack_path, "add_results"),
    );
    assert_eq!(
        stack
            .add_stack(stack_history.id, b, b_commit)
            .unwrap()
            .result_id,
        s2
    );
    assert_eq!(
        (
            count(&stack_path, "stacks"),
            count(&stack_path, "add_results")
        ),
        rows_before_repeat
    );

    stack.push_stack(s2).unwrap();
    assert_eq!(objects(&layer_path), objects_after_a);
    assert_eq!(layer.branch_record(b).unwrap().head_commit_id, b_commit);
    assert_eq!(
        layer
            .stack_history_record(stack_history.id)
            .unwrap()
            .head_stack_id,
        s2
    );
    let mapped: Vec<u8> = rusqlite::Connection::open(&layer_path)
        .unwrap()
        .query_row(
            "SELECT result_id FROM add_results WHERE source_id=?1",
            [b.as_slice()],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(mapped, s2.as_slice());
    let mut saw_commit = false;
    layer
        .visit_commits(
            b,
            &mut |kind, ids| {
                assert_eq!(kind, FactKind::Commit);
                layerfs_storage_core::MissingBitmap::from_missing(ids.len(), |_| true)
            },
            &mut |commits| {
                saw_commit |= commits.iter().any(|commit| commit.id == b_commit);
                Ok(())
            },
        )
        .unwrap();
    assert!(saw_commit);
    drop(branch);
    drop(stack);
    drop(layer);
    std::fs::remove_dir_all(run).unwrap();
}

fn create_installs(
    store: &BranchStore,
    layer_history: layerfs_storage_core::LayerHistoryId,
    layer: layerfs_storage_core::LayerId,
    stack: Option<(
        layerfs_storage_core::StackHistoryId,
        layerfs_storage_core::StackId,
    )>,
    changes: &[Change],
) -> Vec<(BranchId, CommitId)> {
    (0..INSTALLATIONS)
        .map(|_| {
            let branch = match stack {
                Some((history, stack)) => store.create_branch_from_stack(history, stack).unwrap(),
                None => store
                    .create_branch_from_layer(layer_history, layer)
                    .unwrap(),
            };
            let commit = match store
                .commit(branch.id, branch.head_commit_id, changes)
                .unwrap()
            {
                RefOutcome::Created(id) => id,
                _ => panic!(),
            };
            (branch.id, commit)
        })
        .collect()
}

fn package_changes() -> Vec<Change> {
    let mut changes = [
        "installed",
        "installed/bin",
        "installed/lib",
        "installed/share",
        "installed/share/doc",
        "installed/etc",
    ]
    .into_iter()
    .map(|path| Change::Mkdir {
        path: path.into(),
        mode: 0o755,
    })
    .collect::<Vec<_>>();
    for (directory, files, bytes) in [
        ("bin", 8, 96 * 1024),
        ("lib", 24, 128 * 1024),
        ("share/doc", 12, 12 * 1024),
        ("etc", 8, 4 * 1024),
    ] {
        for index in 0..files {
            let path = format!("installed/{directory}/package-{index:02}");
            changes.push(Change::Write {
                bytes: package_bytes(&path, bytes),
                path,
                mode: if directory == "bin" { 0o755 } else { 0o644 },
            });
        }
    }
    changes.push(Change::HardLink {
        source: "installed/bin/package-00".into(),
        target: "installed/bin/package-main".into(),
    });
    changes.push(Change::Symlink {
        path: "installed/current-library".into(),
        target: b"lib/package-00".to_vec(),
    });
    changes
}

fn small_install_changes() -> Vec<Change> {
    vec![
        Change::Mkdir {
            path: "installed".into(),
            mode: 0o755,
        },
        Change::Write {
            path: "installed/package".into(),
            bytes: b"one deterministic installation payload".to_vec(),
            mode: 0o644,
        },
    ]
}

fn package_bytes(label: &str, length: usize) -> Vec<u8> {
    let mut state = label
        .bytes()
        .fold(0x4c41_5945_5246_5346_u64, |state, byte| {
            state.rotate_left(7) ^ u64::from(byte)
        });
    (0..length)
        .map(|index| {
            state ^= state.wrapping_shl(7);
            state ^= state.wrapping_shr(9);
            state ^= state.wrapping_shl(8);
            (state as u8) ^ (index as u8).rotate_left((index % 7) as u32)
        })
        .collect()
}

#[derive(Debug)]
struct DbAccounting {
    object_rows: u64,
    object_payload_bytes: u64,
    metadata_rows: u64,
    total_db_bytes: u64,
}

fn assert_accounting(
    topology: &str,
    role: &str,
    path: &std::path::Path,
    expected_objects: &BTreeMap<Vec<u8>, u64>,
    expected_metadata_rows: u64,
) {
    let actual = accounting(path);
    let expected_payload_bytes = expected_objects.values().sum::<u64>();
    assert_eq!(actual.object_rows, expected_objects.len() as u64);
    assert_eq!(actual.object_payload_bytes, expected_payload_bytes);
    assert_eq!(actual.metadata_rows, expected_metadata_rows);
    assert!(actual.total_db_bytes >= actual.object_payload_bytes);
    eprintln!(
        "TEN_INSTALL_STORAGE topology={topology} database={role} installations={INSTALLATIONS} object_rows={} object_payload_bytes={} metadata_rows={} total_db_bytes={}",
        actual.object_rows,
        actual.object_payload_bytes,
        actual.metadata_rows,
        actual.total_db_bytes,
    );
}

fn accounting(path: &std::path::Path) -> DbAccounting {
    let connection = rusqlite::Connection::open(path).unwrap();
    let (object_rows, object_payload_bytes) = connection
        .query_row(
            "SELECT count(*),coalesce(sum(length(bytes)),0) FROM objects",
            [],
            |row| Ok((row.get::<_, i64>(0)? as u64, row.get::<_, i64>(1)? as u64)),
        )
        .unwrap();
    let metadata_rows = [
        "branches",
        "commits",
        "layer_histories",
        "layers",
        "stack_histories",
        "stacks",
        "add_results",
    ]
    .into_iter()
    .filter(|table| {
        connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE type='table' AND name=?1)",
                [table],
                |row| row.get::<_, bool>(0),
            )
            .unwrap()
    })
    .map(|table| {
        connection
            .query_row(&format!("SELECT count(*) FROM {table}"), [], |row| {
                row.get::<_, i64>(0)
            })
            .unwrap() as u64
    })
    .sum();
    drop(connection);
    DbAccounting {
        object_rows,
        object_payload_bytes,
        metadata_rows,
        total_db_bytes: database_files(path),
    }
}

fn database_files(path: &std::path::Path) -> u64 {
    let base = path.as_os_str().to_string_lossy();
    [
        base.to_string(),
        format!("{base}-wal"),
        format!("{base}-shm"),
    ]
    .into_iter()
    .filter_map(|path| std::fs::metadata(path).ok())
    .map(|metadata| metadata.len())
    .sum()
}

fn count(path: &std::path::Path, table: &str) -> u64 {
    rusqlite::Connection::open(path)
        .unwrap()
        .query_row(&format!("SELECT count(*) FROM {table}"), [], |row| {
            row.get::<_, i64>(0)
        })
        .unwrap() as u64
}

fn objects(path: &std::path::Path) -> BTreeMap<Vec<u8>, u64> {
    let connection = rusqlite::Connection::open(path).unwrap();
    let mut statement = connection
        .prepare("SELECT object_id,length(bytes) FROM objects ORDER BY object_id")
        .unwrap();
    statement
        .query_map([], |row| Ok((row.get(0)?, row.get::<_, i64>(1)? as u64)))
        .unwrap()
        .collect::<std::result::Result<_, _>>()
        .unwrap()
}

fn reachable(source: &dyn ObjectSource, root: layerfs_core::ObjectId) -> BTreeMap<Vec<u8>, u64> {
    dependency_order(source, root)
        .unwrap()
        .into_iter()
        .map(|id| {
            let bytes = source.read_object(id).unwrap();
            (id.as_bytes().to_vec(), bytes.len() as u64)
        })
        .collect()
}

fn union(left: &BTreeMap<Vec<u8>, u64>, right: &BTreeMap<Vec<u8>, u64>) -> BTreeMap<Vec<u8>, u64> {
    let mut union = left.clone();
    for (id, bytes) in right {
        assert!(union
            .insert(id.clone(), *bytes)
            .is_none_or(|old| old == *bytes));
    }
    union
}

fn run_dir(name: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!(
        "layerfs-dedup-{name}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&path).unwrap();
    path
}
