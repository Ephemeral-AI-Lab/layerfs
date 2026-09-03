use layerfs_layerstack_store::{
    set_transaction_failure_at, EntityName, LayerStackInitialization, LayerStackStore,
    LocalForkSource, ObjectBuffer,
};
use layerfs_workspace::{
    CreateWorkspaceSession, EndWorkspaceMode, WorkspaceCommitResult, WorkspaceFileRangeEdit,
    WorkspaceFileReplacement, WorkspacePlacement, WorkspaceProjection, WorkspaceSession,
    Workspaces,
};

fn fixture(
    label: &str,
    bytes: &[u8],
) -> (
    std::path::PathBuf,
    Workspaces,
    layerfs_layerstack_store::BranchId,
    LayerStackStore,
) {
    let root = std::env::temp_dir().join(format!(
        "layerfs-file-edit-{label}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let source = root.join("source");
    std::fs::create_dir_all(&source).unwrap();
    std::fs::write(source.join("file"), bytes).unwrap();
    let store = LayerStackStore::create(root.join("store.sqlite")).unwrap();
    let layer = store
        .initialize_layerstack(
            EntityName::new("project").unwrap(),
            LayerStackInitialization::Directory(source),
        )
        .unwrap()
        .genesis_layer_id;
    let branch = store
        .fork_branch(
            EntityName::new("main").unwrap(),
            LocalForkSource::Layer { layer_id: layer },
        )
        .unwrap();
    let workspaces = Workspaces::new(root.join("runtime"), store.clone()).unwrap();
    (root.clone(), workspaces, branch, store)
}

fn open_session(
    root: &std::path::Path,
    workspaces: &Workspaces,
    branch: layerfs_layerstack_store::BranchId,
    name: &str,
) -> (WorkspaceSession, std::path::PathBuf) {
    let mount = root.join(name);
    let session = workspaces
        .create_workspace_session(CreateWorkspaceSession {
            branch_id: branch,
            placement: WorkspacePlacement::Host {
                root: mount.clone(),
            },
            projection: Some(WorkspaceProjection::Materialize),
        })
        .unwrap();
    (session, mount)
}

fn edit(
    workspaces: &Workspaces,
    session: &WorkspaceSession,
    start: u64,
    delete_len: u64,
    replacement: WorkspaceFileReplacement,
) -> layerfs_workspace::WorkspaceResult<()> {
    workspaces.edit_workspace_file_range(WorkspaceFileRangeEdit {
        workspace_id: session.id,
        path: "file".into(),
        start,
        delete_len,
        replacement,
    })
}

#[test]
fn group_1_range_piece_eof_noop_and_repeated_boundaries() {
    let (root, workspaces, branch, _) = fixture("ranges", b"abcdef");
    let (session, mount) = open_session(&root, &workspaces, branch, "mount");
    edit(
        &workspaces,
        &session,
        0,
        0,
        WorkspaceFileReplacement::Inline(b"P".to_vec()),
    )
    .unwrap();
    edit(
        &workspaces,
        &session,
        3,
        2,
        WorkspaceFileReplacement::Inline(b"XYZ".to_vec()),
    )
    .unwrap();
    edit(
        &workspaces,
        &session,
        8,
        0,
        WorkspaceFileReplacement::Zero(2),
    )
    .unwrap();
    assert_eq!(std::fs::read(mount.join("file")).unwrap(), b"PabXYZef\0\0");
    assert!(edit(
        &workspaces,
        &session,
        11,
        0,
        WorkspaceFileReplacement::Inline(vec![])
    )
    .is_err());
    assert!(workspaces
        .edit_workspace_file_ranges(vec![
            WorkspaceFileRangeEdit {
                workspace_id: session.id,
                path: "file".into(),
                start: 0,
                delete_len: 1,
                replacement: WorkspaceFileReplacement::Inline(b"X".to_vec()),
            },
            WorkspaceFileRangeEdit {
                workspace_id: session.id,
                path: "file".into(),
                start: 99,
                delete_len: 0,
                replacement: WorkspaceFileReplacement::Inline(b"Y".to_vec()),
            },
        ])
        .is_err());
    assert_eq!(std::fs::read(mount.join("file")).unwrap(), b"PabXYZef\0\0");
    workspaces
        .end_workspace_session(session.id, EndWorkspaceMode::Discard)
        .unwrap();
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn group_2_projection_refresh_commit_and_reopen_are_exact() {
    let (root, workspaces, branch, _) = fixture("projection", b"abcdef");
    let (session, mount) = open_session(&root, &workspaces, branch, "mount");
    edit(
        &workspaces,
        &session,
        2,
        2,
        WorkspaceFileReplacement::Inline(b"XYZ".to_vec()),
    )
    .unwrap();
    assert_eq!(std::fs::read(mount.join("file")).unwrap(), b"abXYZef");
    edit(
        &workspaces,
        &session,
        1,
        1,
        WorkspaceFileReplacement::Zero(3),
    )
    .unwrap();
    assert_eq!(std::fs::read(mount.join("file")).unwrap(), b"a\0\0\0XYZef");
    assert!(matches!(
        workspaces.commit_workspace_session(session.id).unwrap(),
        WorkspaceCommitResult::Created { .. }
    ));
    workspaces
        .end_workspace_session(session.id, EndWorkspaceMode::Clean)
        .unwrap();
    let (reopened, reopen) = open_session(&root, &workspaces, branch, "reopen");
    assert_eq!(std::fs::read(reopen.join("file")).unwrap(), b"a\0\0\0XYZef");
    workspaces
        .end_workspace_session(reopened.id, EndWorkspaceMode::Clean)
        .unwrap();
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn group_3_hard_link_aliases_share_one_edited_inode() {
    let (root, workspaces, _branch, _) = fixture("aliases", b"abcdef");
    let source = root.join("source");
    std::fs::hard_link(source.join("file"), source.join("alias")).unwrap();
    // Reinitialize after adding the alias so the Store sees one inode with two names.
    drop(workspaces);
    std::fs::remove_file(root.join("store.sqlite")).unwrap();
    let store = LayerStackStore::create(root.join("store.sqlite")).unwrap();
    let layer = store
        .initialize_layerstack(
            EntityName::new("aliases").unwrap(),
            LayerStackInitialization::Directory(source),
        )
        .unwrap()
        .genesis_layer_id;
    let branch = store
        .fork_branch(
            EntityName::new("main").unwrap(),
            LocalForkSource::Layer { layer_id: layer },
        )
        .unwrap();
    let workspaces = Workspaces::new(root.join("runtime-2"), store).unwrap();
    let (session, mount) = open_session(&root, &workspaces, branch, "mount");
    edit(
        &workspaces,
        &session,
        1,
        3,
        WorkspaceFileReplacement::Inline(b"X".to_vec()),
    )
    .unwrap();
    assert_eq!(std::fs::read(mount.join("file")).unwrap(), b"aXef");
    assert_eq!(std::fs::read(mount.join("alias")).unwrap(), b"aXef");
    workspaces
        .end_workspace_session(session.id, EndWorkspaceMode::Discard)
        .unwrap();
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn group_4_invalid_type_range_overflow_and_limits_are_atomic() {
    let (root, workspaces, branch, _) = fixture("reject", b"abcdef");
    let (session, mount) = open_session(&root, &workspaces, branch, "mount");
    let before = std::fs::read(mount.join("file")).unwrap();
    assert!(edit(
        &workspaces,
        &session,
        7,
        0,
        WorkspaceFileReplacement::Inline(vec![])
    )
    .is_err());
    assert!(edit(
        &workspaces,
        &session,
        u64::MAX,
        1,
        WorkspaceFileReplacement::Inline(vec![])
    )
    .is_err());
    assert!(edit(
        &workspaces,
        &session,
        0,
        0,
        WorkspaceFileReplacement::Inline(vec![0; 1024 * 1024 + 1])
    )
    .is_err());
    assert_eq!(std::fs::read(mount.join("file")).unwrap(), before);
    workspaces
        .end_workspace_session(session.id, EndWorkspaceMode::Discard)
        .unwrap();
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn group_5_commit_publication_is_exactly_once_and_retry_is_up_to_date() {
    let (root, workspaces, branch, _) = fixture("retry", b"abcdef");
    let (session, _) = open_session(&root, &workspaces, branch, "mount");
    edit(
        &workspaces,
        &session,
        0,
        0,
        WorkspaceFileReplacement::Inline(b"P".to_vec()),
    )
    .unwrap();
    set_transaction_failure_at(Some(1));
    assert!(workspaces.commit_workspace_session(session.id).is_err());
    set_transaction_failure_at(None);
    assert!(matches!(
        workspaces.commit_workspace_session(session.id).unwrap(),
        WorkspaceCommitResult::Created { .. }
    ));
    assert!(matches!(
        workspaces.commit_workspace_session(session.id).unwrap(),
        WorkspaceCommitResult::UpToDate { .. }
    ));
    workspaces
        .end_workspace_session(session.id, EndWorkspaceMode::Clean)
        .unwrap();
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn group_6_discard_mixed_inline_zero_and_spool_state_leaves_no_durable_change() {
    let (root, workspaces, branch, _) = fixture("discard", b"abcdef");
    let (session, mount) = open_session(&root, &workspaces, branch, "mount");
    edit(
        &workspaces,
        &session,
        1,
        2,
        WorkspaceFileReplacement::Inline(b"X".to_vec()),
    )
    .unwrap();
    edit(
        &workspaces,
        &session,
        2,
        0,
        WorkspaceFileReplacement::Zero(2),
    )
    .unwrap();
    std::fs::write(mount.join("file"), b"ordinary-spool").unwrap();
    workspaces
        .end_workspace_session(session.id, EndWorkspaceMode::Discard)
        .unwrap();
    let (reopened, reopen) = open_session(&root, &workspaces, branch, "reopen");
    assert_eq!(std::fs::read(reopen.join("file")).unwrap(), b"abcdef");
    workspaces
        .end_workspace_session(reopened.id, EndWorkspaceMode::Clean)
        .unwrap();
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn group_7_stale_head_and_posix_owner_composition_are_exact() {
    let (root, workspaces, branch, store) = fixture("composition", b"abcdef");
    let (first, first_mount) = open_session(&root, &workspaces, branch, "first");
    std::fs::write(first_mount.join("file"), b"ABcdef").unwrap();
    edit(
        &workspaces,
        &first,
        2,
        0,
        WorkspaceFileReplacement::Inline(b"X".to_vec()),
    )
    .unwrap();
    assert!(matches!(
        workspaces.commit_workspace_session(first.id).unwrap(),
        WorkspaceCommitResult::Created { .. }
    ));
    workspaces
        .end_workspace_session(first.id, EndWorkspaceMode::Clean)
        .unwrap();
    let (stale, _) = open_session(&root, &workspaces, branch, "stale");
    edit(
        &workspaces,
        &stale,
        0,
        0,
        WorkspaceFileReplacement::Inline(b"S".to_vec()),
    )
    .unwrap();
    let pinned = store.pin_branch(branch).unwrap();
    let mut objects = ObjectBuffer::new(&pinned.reader).unwrap();
    let changed = layerfs_content::filesystem::set_mtime(
        &mut objects,
        pinned.root,
        &layerfs_content::CanonicalPath::new("file").unwrap(),
        1,
        0,
    )
    .unwrap();
    let built = objects
        .finish(changed.root(), changed.counters().rope.cdc_bytes_scanned)
        .unwrap();
    store
        .commit_candidate(
            &pinned.branch,
            pinned.root,
            pinned.branch.base_layer_id,
            built,
        )
        .unwrap();
    assert!(matches!(
        workspaces.commit_workspace_session(stale.id).unwrap(),
        WorkspaceCommitResult::HeadMoved { .. }
    ));
    workspaces
        .end_workspace_session(stale.id, EndWorkspaceMode::Discard)
        .unwrap();
    let (second, second_mount) = open_session(&root, &workspaces, branch, "second");
    edit(
        &workspaces,
        &second,
        3,
        0,
        WorkspaceFileReplacement::Inline(b"Y".to_vec()),
    )
    .unwrap();
    std::fs::write(second_mount.join("file"), b"ABXYcdeZ").unwrap();
    assert!(matches!(
        workspaces.commit_workspace_session(second.id).unwrap(),
        WorkspaceCommitResult::Created { .. }
    ));
    workspaces
        .end_workspace_session(second.id, EndWorkspaceMode::Clean)
        .unwrap();
    let (reopened, reopen) = open_session(&root, &workspaces, branch, "reopen");
    assert_eq!(std::fs::read(reopen.join("file")).unwrap(), b"ABXYcdeZ");
    workspaces
        .end_workspace_session(reopened.id, EndWorkspaceMode::Clean)
        .unwrap();
    std::fs::remove_dir_all(root).unwrap();
}
