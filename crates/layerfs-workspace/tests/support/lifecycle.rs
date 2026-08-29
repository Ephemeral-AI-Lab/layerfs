use crate::{
    CreateWorkspaceSession, EndWorkspaceMode, NonEmpty, OutputStream, ResourcePolicy, Workspace,
    WorkspaceCommitResult, WorkspaceError, WorkspacePlacement, WorkspaceProjection, WorkspaceState,
    Workspaces, ROOT,
};
use layerfs_branch_store::BranchStore;
use layerfs_content::filesystem::ContentChange;
use layerfs_layer_store::LayerStore;
use layerfs_storage::RefOutcome;
use std::sync::Arc;

fn fixture(name: &str) -> (std::path::PathBuf, Workspace) {
    fixture_with_policy(
        name,
        ResourcePolicy {
            max_spool_bytes: 1024 * 1024,
            ..ResourcePolicy::default()
        },
    )
}

fn fixture_with_policy(name: &str, policy: ResourcePolicy) -> (std::path::PathBuf, Workspace) {
    let root = std::env::temp_dir().join(format!(
        "layerfs-workspace-{name}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&root).unwrap();
    let layer = Arc::new(LayerStore::create(root.join("layer.sqlite")).unwrap());
    let (_history, genesis) = layer
        .initialize(layerfs_storage::LayerInitialization::Empty)
        .unwrap();
    let store = BranchStore::create(root.join("branch.sqlite"), layer).unwrap();
    let branch = store
        .create_branch(layerfs_storage::BranchSource::Layer(genesis.id))
        .unwrap();
    let workspace =
        Workspace::open_with_policy(store, branch.id, root.join("spool"), policy).unwrap();
    (root, workspace)
}

#[test]
fn overlay_open_unlink_and_commit_are_transient() {
    let (root, mut workspace) = fixture("lifecycle");
    let file = workspace.create_file(ROOT, b"file", 0o644).unwrap();
    workspace.pin(file.node, false).unwrap();
    workspace.write(file.node, 0, b"kept-open").unwrap();
    workspace.unlink(ROOT, b"file", false).unwrap();
    assert_eq!(workspace.read(file.node, 0, 32).unwrap(), b"kept-open");
    workspace.unpin(file.node).unwrap();
    let final_file = workspace.create_file(ROOT, b"final", 0o600).unwrap();
    workspace.write(final_file.node, 0, b"final-bytes").unwrap();
    assert!(matches!(
        workspace.commit().unwrap(),
        RefOutcome::Created(_)
    ));
    assert_eq!(workspace.state, WorkspaceState::Committed);
    assert!(std::fs::read_dir(root.join("spool"))
        .unwrap()
        .all(|entry| !entry.unwrap().path().to_string_lossy().contains("sqlite")));
    drop(workspace);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn spool_limit_is_reclaimed_after_open_unlink_release() {
    let (root, mut workspace) = fixture_with_policy(
        "spool-limit",
        ResourcePolicy {
            max_spool_bytes: 4,
            ..ResourcePolicy::default()
        },
    );
    let file = workspace.create_file(ROOT, b"first", 0o600).unwrap();
    workspace.pin(file.node, false).unwrap();
    workspace.write(file.node, 0, b"1234").unwrap();
    assert!(workspace.write(file.node, 4, b"5").is_err());
    workspace.unlink(ROOT, b"first", false).unwrap();
    workspace.unpin(file.node).unwrap();
    let next = workspace.create_file(ROOT, b"next", 0o600).unwrap();
    workspace.write(next.node, 0, b"5678").unwrap();
    drop(workspace);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn final_delta_memory_cap_preserves_the_inspectable_workspace() {
    let (root, mut workspace) = fixture_with_policy(
        "final-delta-limit",
        ResourcePolicy {
            max_spool_bytes: 1024 * 1024,
            max_final_delta_memory_bytes: 128,
        },
    );
    let file = workspace.create_file(ROOT, b"file", 0o600).unwrap();
    workspace.write(file.node, 0, b"state").unwrap();
    assert!(matches!(
        workspace.commit(),
        Err(layerfs_storage::StorageError::InvalidInput(
            "workspace final-delta limit"
        ))
    ));
    assert_eq!(workspace.state, WorkspaceState::Active);
    assert_eq!(workspace.read(file.node, 0, 16).unwrap(), b"state");
    drop(workspace);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn append_then_truncate_finalizes_from_the_final_overlay_ranges() {
    let (root, mut workspace) = fixture("append-truncate");
    let file = workspace.create_file(ROOT, b"file", 0o600).unwrap();
    workspace.write(file.node, 0, b"alpha").unwrap();
    workspace.write(file.node, 5, b"-beta").unwrap();
    workspace.truncate(file.node, 7).unwrap();
    assert_eq!(workspace.read(file.node, 0, 32).unwrap(), b"alpha-b");
    assert!(matches!(
        workspace.commit().unwrap(),
        RefOutcome::Created(_)
    ));
    drop(workspace);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn sparse_zero_writes_preserve_exact_overlap_semantics() {
    use crate::cow_tree::{Data, FileData};
    use std::os::unix::fs::MetadataExt;

    let (root, mut workspace) = fixture("sparse-zero");
    let sparse = workspace.create_file(ROOT, b"sparse", 0o600).unwrap();
    workspace.write_zero(sparse.node, 0, 1024 * 1024).unwrap();
    let Data::File(FileData::Overlay { spool, .. }) = &workspace.nodes[&sparse.node].data else {
        panic!("overlay")
    };
    assert!(std::fs::metadata(spool).unwrap().blocks() * 512 < 64 * 1024);
    assert_eq!(workspace.read(sparse.node, 1024, 4).unwrap(), [0; 4]);

    let overlap = workspace.create_file(ROOT, b"overlap", 0o600).unwrap();
    workspace.write(overlap.node, 0, b"abcdef").unwrap();
    workspace.write_zero(overlap.node, 1, 2).unwrap();
    assert_eq!(workspace.read(overlap.node, 0, 6).unwrap(), b"a\0\0def");
    assert!(matches!(
        workspace.commit().unwrap(),
        RefOutcome::Created(_)
    ));
    drop(workspace);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn sparse_overlay_commits_a_64_mib_base_without_hydrating_it() {
    use std::os::unix::fs::MetadataExt;

    let root = std::env::temp_dir().join(format!(
        "layerfs-workspace-large-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&root).unwrap();
    let layer = Arc::new(LayerStore::create(root.join("layer.sqlite")).unwrap());
    let (_history, genesis) = layer
        .initialize(layerfs_storage::LayerInitialization::Empty)
        .unwrap();
    let store = BranchStore::create(root.join("branch.sqlite"), layer).unwrap();
    let branch = store
        .create_branch(layerfs_storage::BranchSource::Layer(genesis.id))
        .unwrap();
    let RefOutcome::Created(_head) = store
        .commit(
            branch.id,
            branch.head_commit_id,
            &[ContentChange::Write {
                path: "large".into(),
                bytes: vec![b'a'; 64 * 1024 * 1024],
                mode: 0o644,
            }],
        )
        .unwrap()
    else {
        panic!("expected Commit")
    };
    let mut workspace = Workspace::open_with_policy(
        store.clone(),
        branch.id,
        root.join("spool"),
        ResourcePolicy {
            max_spool_bytes: 1024 * 1024,
            ..ResourcePolicy::default()
        },
    )
    .unwrap();
    let file = workspace.lookup(ROOT, b"large").unwrap();
    workspace.write(file.node, 32 * 1024 * 1024, b"z").unwrap();
    let spool = std::fs::read_dir(root.join("spool"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let metadata = std::fs::metadata(spool).unwrap();
    assert_eq!(metadata.len(), 64 * 1024 * 1024);
    assert!(metadata.blocks() * 512 < 1024 * 1024);
    assert!(matches!(
        workspace.commit().unwrap(),
        RefOutcome::Created(_)
    ));
    let mut reopened = Workspace::open(store, branch.id, root.join("reopen-spool")).unwrap();
    let reopened_file = reopened.lookup(ROOT, b"large").unwrap();
    assert_eq!(
        reopened
            .read(reopened_file.node, 32 * 1024 * 1024 - 1, 3)
            .unwrap(),
        b"aza"
    );
    drop(reopened);
    drop(workspace);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn directory_parent_and_descendant_rename_are_correct() {
    let (root, mut workspace) = fixture("directory-parent");
    let parent = workspace.mkdir(ROOT, b"parent", 0o755).unwrap();
    let child = workspace.mkdir(parent.node, b"child", 0o755).unwrap();
    let entries = workspace.readdir(child.node).unwrap();
    assert!(entries
        .iter()
        .any(|(node, _, name)| *node == parent.node && name == b".."));
    assert!(workspace
        .rename(ROOT, b"parent", child.node, b"loop", false)
        .is_err());
    drop(workspace);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn final_delta_identity_ignores_workspace_operation_history() {
    let root = std::env::temp_dir().join(format!(
        "layerfs-workspace-final-state-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&root).unwrap();
    let layer = Arc::new(LayerStore::create(root.join("layer.sqlite")).unwrap());
    let (_history, genesis) = layer
        .initialize(layerfs_storage::LayerInitialization::Empty)
        .unwrap();
    let store = BranchStore::create(root.join("branch.sqlite"), layer).unwrap();
    let left_branch = store
        .create_branch(layerfs_storage::BranchSource::Layer(genesis.id))
        .unwrap();
    let right_branch = store
        .create_branch(layerfs_storage::BranchSource::Layer(genesis.id))
        .unwrap();
    let mut left = Workspace::open(store.clone(), left_branch.id, root.join("left-spool")).unwrap();
    let mut right =
        Workspace::open(store.clone(), right_branch.id, root.join("right-spool")).unwrap();

    let left_file = left.create_file(ROOT, b"file", 0o640).unwrap();
    left.write(left_file.node, 0, b"final bytes").unwrap();
    left.link(left_file.node, ROOT, b"alias").unwrap();

    let temporary = right.create_file(ROOT, b"temporary", 0o600).unwrap();
    right.write(temporary.node, 0, b"discarded").unwrap();
    right.unlink(ROOT, b"temporary", false).unwrap();
    let right_file = right.create_file(ROOT, b"file", 0o640).unwrap();
    right.write(right_file.node, 0, b"draft").unwrap();
    right.truncate(right_file.node, 0).unwrap();
    right.write(right_file.node, 0, b"final bytes").unwrap();
    right.link(right_file.node, ROOT, b"alias").unwrap();
    right.rename(ROOT, b"alias", ROOT, b"moved", false).unwrap();
    right.rename(ROOT, b"moved", ROOT, b"alias", false).unwrap();

    assert!(matches!(left.commit().unwrap(), RefOutcome::Created(_)));
    assert!(matches!(right.commit().unwrap(), RefOutcome::Created(_)));
    let left_root = store.root(left_branch.id).unwrap();
    let right_root = store.root(right_branch.id).unwrap();
    assert_eq!(left_root, right_root);
    assert_eq!(
        layerfs_storage::dependency_order(&store, left_root).unwrap(),
        layerfs_storage::dependency_order(&store, right_root).unwrap()
    );
    assert_eq!(left.read(left_file.node, 0, 64).unwrap(), b"final bytes");
    assert!(left.write(left_file.node, 0, b"read only").is_err());

    drop(left);
    drop(right);
    drop(store);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn head_moved_preserves_final_workspace_delta() {
    let root = std::env::temp_dir().join(format!(
        "layerfs-workspace-head-moved-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&root).unwrap();
    let layer = Arc::new(LayerStore::create(root.join("layer.sqlite")).unwrap());
    let (_history, genesis) = layer
        .initialize(layerfs_storage::LayerInitialization::Empty)
        .unwrap();
    let store = BranchStore::create(root.join("branch.sqlite"), layer).unwrap();
    let branch = store
        .create_branch(layerfs_storage::BranchSource::Layer(genesis.id))
        .unwrap();
    let mut winner = Workspace::open(store.clone(), branch.id, root.join("winner-spool")).unwrap();
    let mut loser = Workspace::open(store.clone(), branch.id, root.join("loser-spool")).unwrap();
    let winner_file = winner.create_file(ROOT, b"winner", 0o600).unwrap();
    winner.write(winner_file.node, 0, b"winner").unwrap();
    let loser_file = loser.create_file(ROOT, b"loser", 0o600).unwrap();
    loser.write(loser_file.node, 0, b"preserved").unwrap();
    winner.commit().unwrap();

    assert!(matches!(
        loser.commit(),
        Err(layerfs_storage::StorageError::CommitHeadMoved(_))
    ));
    assert_eq!(loser.state, WorkspaceState::Active);
    assert_eq!(loser.read(loser_file.node, 0, 64).unwrap(), b"preserved");

    drop(loser);
    drop(winner);
    drop(store);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn public_materialized_session_is_ready_detects_dirty_and_commits_explicitly() {
    let root = std::env::temp_dir().join(format!(
        "layerfs-workspace-session-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&root).unwrap();
    let layer = Arc::new(LayerStore::create(root.join("layer.sqlite")).unwrap());
    let (_, genesis) = layer
        .initialize(layerfs_storage::LayerInitialization::Empty)
        .unwrap();
    let branch_path = root.join("branch.sqlite");
    let store = BranchStore::create(&branch_path, layer).unwrap();
    let branch = store
        .create_branch(layerfs_storage::BranchSource::Layer(genesis.id))
        .unwrap();
    let workspaces = Workspaces::new(root.join("runtime"), [store.clone()]).unwrap();
    let output = root.join("output");
    let commits_before = row_count(&branch_path, "commits");
    let session = workspaces
        .create_workspace_session(CreateWorkspaceSession {
            branch_id: branch.id,
            placement: WorkspacePlacement::Host {
                root: output.clone(),
            },
            projection: Some(WorkspaceProjection::Materialize),
        })
        .unwrap();
    let session_runtime = root.join("runtime/workspaces").join(session.id.to_string());
    assert!(session_runtime.is_dir());
    assert!(output.is_dir());
    assert_eq!(row_count(&branch_path, "commits"), commits_before);
    std::fs::write(output.join("final"), b"portable path").unwrap();
    assert!(workspaces.diff(session.id).unwrap().dirty);
    assert!(matches!(
        workspaces.end_workspace_session(session.id, EndWorkspaceMode::Clean),
        Err(WorkspaceError::WorkspaceDirty)
    ));
    assert!(matches!(
        workspaces.commit_workspace_session(session.id).unwrap(),
        WorkspaceCommitResult::Created { .. }
    ));
    assert_eq!(
        store.read_path(branch.id, "final").unwrap(),
        b"portable path"
    );
    assert!(std::fs::metadata(output.join("final"))
        .unwrap()
        .permissions()
        .readonly());
    workspaces
        .end_workspace_session(session.id, EndWorkspaceMode::Clean)
        .unwrap();
    assert!(!output.exists());
    assert!(!session_runtime.exists());
    let retained = workspaces.session(session.id).unwrap();
    assert_eq!(retained.session.state, WorkspaceState::Ended);
    assert!(!workspaces.diff(session.id).unwrap().dirty);
    assert_eq!(workspaces.sessions().unwrap().len(), 1);

    let unchanged_root = root.join("unchanged");
    let unchanged = workspaces
        .create_workspace_session(CreateWorkspaceSession {
            branch_id: branch.id,
            placement: WorkspacePlacement::Host {
                root: unchanged_root,
            },
            projection: Some(WorkspaceProjection::Materialize),
        })
        .unwrap();
    let before = row_count(&branch_path, "commits");
    assert!(matches!(
        workspaces.commit_workspace_session(unchanged.id).unwrap(),
        WorkspaceCommitResult::UpToDate { .. }
    ));
    assert_eq!(row_count(&branch_path, "commits"), before);
    workspaces
        .end_workspace_session(unchanged.id, EndWorkspaceMode::Clean)
        .unwrap();

    let discarded_root = root.join("discarded");
    let discarded = workspaces
        .create_workspace_session(CreateWorkspaceSession {
            branch_id: branch.id,
            placement: WorkspacePlacement::Host {
                root: discarded_root.clone(),
            },
            projection: Some(WorkspaceProjection::Materialize),
        })
        .unwrap();
    std::fs::write(discarded_root.join("discarded"), b"not committed").unwrap();
    let ended = workspaces
        .end_workspace_session(discarded.id, EndWorkspaceMode::Discard)
        .unwrap();
    assert!(ended.discarded);
    assert_eq!(
        workspaces.session(discarded.id).unwrap().session.state,
        WorkspaceState::Ended,
    );
    assert!(!workspaces.diff(discarded.id).unwrap().dirty);
    assert!(!discarded_root.exists());

    drop(workspaces);
    drop(store);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn clean_end_uses_final_state_not_cancelled_mutation_history() {
    let root = run_dir("clean-final-state");
    let layer = Arc::new(LayerStore::create(root.join("layer.sqlite")).unwrap());
    let (_, genesis) = layer
        .initialize(layerfs_storage::LayerInitialization::Empty)
        .unwrap();
    let store = BranchStore::create(root.join("branch.sqlite"), layer).unwrap();
    let branch = store
        .create_branch(layerfs_storage::BranchSource::Layer(genesis.id))
        .unwrap();
    let workspaces = Workspaces::new(root.join("runtime"), [store]).unwrap();
    let output = root.join("output");
    let session = workspaces
        .create_workspace_session(CreateWorkspaceSession {
            branch_id: branch.id,
            placement: WorkspacePlacement::Host {
                root: output.clone(),
            },
            projection: Some(WorkspaceProjection::Materialize),
        })
        .unwrap();
    let root_mtime = std::fs::metadata(&output).unwrap().modified().unwrap();
    std::fs::write(output.join("cancelled"), b"transient").unwrap();
    std::fs::remove_file(output.join("cancelled")).unwrap();
    std::fs::File::open(&output)
        .unwrap()
        .set_times(std::fs::FileTimes::new().set_modified(root_mtime))
        .unwrap();
    assert!(!workspaces.diff(session.id).unwrap().dirty);
    workspaces
        .end_workspace_session(session.id, EndWorkspaceMode::Clean)
        .unwrap();
    std::fs::remove_dir_all(root).unwrap();
}

fn row_count(path: &std::path::Path, table: &str) -> u64 {
    rusqlite::Connection::open(path)
        .unwrap()
        .query_row(&format!("SELECT count(*) FROM {table}"), [], |row| {
            row.get::<_, i64>(0)
        })
        .unwrap() as u64
}

fn run_dir(name: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!(
        "layerfs-workspace-{name}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&root).unwrap();
    root
}

#[test]
fn execution_is_direct_bounded_and_blocks_commit_until_exit() {
    use std::ffi::OsString;

    let root = std::env::temp_dir().join(format!(
        "layerfs-workspace-exec-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&root).unwrap();
    let layer = Arc::new(LayerStore::create(root.join("layer.sqlite")).unwrap());
    let (_, genesis) = layer
        .initialize(layerfs_storage::LayerInitialization::Empty)
        .unwrap();
    let store = BranchStore::create(root.join("branch.sqlite"), layer).unwrap();
    let branch = store
        .create_branch(layerfs_storage::BranchSource::Layer(genesis.id))
        .unwrap();
    let workspaces = Workspaces::new(root.join("runtime"), [store.clone()]).unwrap();
    let session = workspaces
        .create_workspace_session(CreateWorkspaceSession {
            branch_id: branch.id,
            placement: WorkspacePlacement::Host {
                root: root.join("output"),
            },
            projection: Some(WorkspaceProjection::Materialize),
        })
        .unwrap();
    let execution = workspaces
        .exec(
            session.id,
            NonEmpty::new(vec![
                OsString::from("/bin/sh"),
                OsString::from("-c"),
                OsString::from(
                    "printf stdout; printf stderr >&2; printf committed > result; sleep 5 & exit 0",
                ),
            ])
            .unwrap(),
        )
        .unwrap();
    let started = std::time::Instant::now();
    assert!(matches!(
        workspaces.commit_workspace_session(session.id),
        Err(WorkspaceError::WorkspaceBusy)
    ));
    assert!(started.elapsed() < std::time::Duration::from_millis(500));
    let reader = workspaces.output(execution.id).unwrap();
    loop {
        let page = reader.read(0, true).unwrap();
        if page.chunks.iter().any(|chunk| chunk.bytes == b"stdout")
            && page.chunks.iter().any(|chunk| chunk.bytes == b"stderr")
        {
            break;
        }
    }
    let stopped_at = std::time::Instant::now();
    workspaces.stop(execution.id).unwrap();

    let page = loop {
        let page = reader.read(0, true).unwrap();
        if page.exited {
            break page;
        }
    };
    assert!(stopped_at.elapsed() < std::time::Duration::from_secs(2));
    assert!(page
        .chunks
        .iter()
        .any(|chunk| chunk.stream == OutputStream::Stdout && chunk.bytes == b"stdout"));
    assert!(page
        .chunks
        .iter()
        .any(|chunk| chunk.stream == OutputStream::Stderr && chunk.bytes == b"stderr"));
    assert!(page.receipt.unwrap().stopped);
    assert!(matches!(
        workspaces.commit_workspace_session(session.id).unwrap(),
        WorkspaceCommitResult::Created { .. }
    ));
    assert_eq!(store.read_path(branch.id, "result").unwrap(), b"committed");
    workspaces
        .end_workspace_session(session.id, EndWorkspaceMode::Clean)
        .unwrap();
    assert!(reader.read(0, false).unwrap().receipt.is_some());
    let retained = workspaces.session(session.id).unwrap();
    assert_eq!(retained.session.state, WorkspaceState::Ended);
    assert_eq!(retained.executions.len(), 1);
    assert!(retained.executions[0].receipt.is_some());
    let retained_output = workspaces
        .output(execution.id)
        .unwrap()
        .read(0, false)
        .unwrap();
    assert!(retained_output.receipt.is_some());
    assert!(retained_output
        .chunks
        .iter()
        .any(|chunk| chunk.bytes == b"stdout"));

    drop(workspaces);
    drop(store);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn workspace_public_manifest_has_three_lifecycle_three_execution_and_four_reads() {
    let _: fn(
        &Workspaces,
        CreateWorkspaceSession,
    ) -> crate::WorkspaceResult<crate::WorkspaceSession> = Workspaces::create_workspace_session;
    let _: fn(
        &Workspaces,
        crate::WorkspaceSessionId,
    ) -> crate::WorkspaceResult<WorkspaceCommitResult> = Workspaces::commit_workspace_session;
    let _: fn(
        &Workspaces,
        crate::WorkspaceSessionId,
        EndWorkspaceMode,
    ) -> crate::WorkspaceResult<crate::WorkspaceEndResult> = Workspaces::end_workspace_session;

    let _: fn(
        &Workspaces,
        crate::WorkspaceSessionId,
        NonEmpty<Vec<std::ffi::OsString>>,
    ) -> crate::WorkspaceResult<crate::WorkspaceExecution> = Workspaces::exec;
    let _: fn(
        &Workspaces,
        crate::WorkspaceSessionId,
    ) -> crate::WorkspaceResult<crate::WorkspaceExecution> = Workspaces::shell;
    let _: fn(&Workspaces, crate::ExecutionId) -> crate::WorkspaceResult<()> = Workspaces::stop;

    let _: fn(&Workspaces) -> crate::WorkspaceResult<Vec<crate::WorkspaceSummary>> =
        Workspaces::sessions;
    let _: fn(
        &Workspaces,
        crate::WorkspaceSessionId,
    ) -> crate::WorkspaceResult<crate::WorkspaceDetail> = Workspaces::session;
    let _: fn(
        &Workspaces,
        crate::WorkspaceSessionId,
    ) -> crate::WorkspaceResult<crate::WorkspaceDiff> = Workspaces::diff;
    let _: fn(&Workspaces, crate::ExecutionId) -> crate::WorkspaceResult<crate::OutputReader> =
        Workspaces::output;
}
