use layerfs_layerstack_store::{
    set_transaction_failure_at, EntityName, LayerStackInitialization, LayerStackStore,
    LocalForkSource, ObjectBuffer,
};
use layerfs_workspace::{
    inject_candidate_failure_once, inject_projection_resume_failure_once, CreateWorkspaceSession,
    EndWorkspaceMode, WorkspaceCommitResult, WorkspaceError, WorkspaceFileRangeEdit,
    WorkspaceFileReplacement, WorkspacePlacement, WorkspaceProjection, WorkspaceSession,
    WorkspaceState, Workspaces,
};
use std::os::unix::fs::MetadataExt;

fn fixture(
    label: &str,
    bytes: &[u8],
) -> (
    std::path::PathBuf,
    Workspaces,
    layerfs_layerstack_store::BranchId,
    LayerStackStore,
) {
    fixture_with(label, |source| {
        std::fs::write(source.join("file"), bytes).unwrap();
    })
}

fn fixture_with(
    label: &str,
    build: impl FnOnce(&std::path::Path),
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
    build(&source);
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
fn whole_file_empty_generation_reclaims_edit_budget() {
    let (root, workspaces, branch, store) = fixture("empty-generation", b"");
    let (session, mount) = open_session(&root, &workspaces, branch, "mount");
    let mut applied = 0usize;
    let mut expected = vec![0; 64];
    let result = (|| -> Result<(), String> {
        edit(
            &workspaces,
            &session,
            0,
            0,
            WorkspaceFileReplacement::Inline(expected.clone()),
        )
        .map_err(|error| format!("initial write: {error:?}"))?;
        applied += 1;
        // Same state transitions as O_TRUNC followed by a 64-byte write,
        // through the public owner API and real presentation refresh.
        for cycle in 1..=2048 {
            edit(
                &workspaces,
                &session,
                0,
                64,
                WorkspaceFileReplacement::Inline(Vec::new()),
            )
            .map_err(|error| format!("cycle={cycle} truncate applied={applied}: {error:?}"))?;
            applied += 1;
            expected = vec![cycle as u8; 64];
            edit(
                &workspaces,
                &session,
                0,
                0,
                WorkspaceFileReplacement::Inline(expected.clone()),
            )
            .map_err(|error| {
                format!(
                    "cycle={cycle} write applied={applied} live_len={}: {error:?}",
                    std::fs::metadata(mount.join("file")).unwrap().len()
                )
            })?;
            applied += 1;
        }
        assert_eq!(std::fs::read(mount.join("file")).unwrap(), expected);
        // The new nonempty generation still accepts exactly 4096 edits.
        // Its final fill above is edit one; these fill the remaining budget.
        for ordinal in 1..4096 {
            expected = vec![ordinal as u8; 64];
            edit(
                &workspaces,
                &session,
                0,
                64,
                WorkspaceFileReplacement::Inline(expected.clone()),
            )
            .map_err(|error| format!("nonempty edit={ordinal}: {error:?}"))?;
        }
        let error = edit(
            &workspaces,
            &session,
            0,
            64,
            WorkspaceFileReplacement::Inline(vec![7; 64]),
        )
        .unwrap_err();
        assert!(matches!(
            error,
            WorkspaceError::Storage(layerfs_layerstack_store::StoreError::InvalidInput(
                "workspace edit limit"
            ))
        ));
        assert_eq!(std::fs::read(mount.join("file")).unwrap(), expected);
        Ok(())
    })();
    if result.is_ok() {
        assert!(matches!(
            workspaces.commit_workspace_session(session.id).unwrap(),
            WorkspaceCommitResult::Created { .. }
        ));
        workspaces
            .end_workspace_session(session.id, EndWorkspaceMode::Clean)
            .unwrap();
        let (reopened, reopened_mount) = open_session(&root, &workspaces, branch, "reopened");
        assert_eq!(
            std::fs::read(reopened_mount.join("file")).unwrap(),
            expected
        );
        workspaces
            .end_workspace_session(reopened.id, EndWorkspaceMode::Clean)
            .unwrap();
    } else {
        eprintln!("empty-generation regression: {:?}", result);
        workspaces
            .end_workspace_session(session.id, EndWorkspaceMode::Discard)
            .unwrap();
    }
    drop(workspaces);
    drop(store);
    std::fs::remove_dir_all(root).unwrap();
    assert!(result.is_ok(), "{result:?}");
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
fn group_1_descending_overlapping_repeated_and_up_to_date_normalize_exactly() {
    let (root, workspaces, branch, _) = fixture("normalize", b"0123456789");
    let (session, mount) = open_session(&root, &workspaces, branch, "mount");
    workspaces
        .edit_workspace_file_ranges(vec![
            WorkspaceFileRangeEdit {
                workspace_id: session.id,
                path: "file".into(),
                start: 8,
                delete_len: 2,
                replacement: WorkspaceFileReplacement::Inline(b"XY".to_vec()),
            },
            WorkspaceFileRangeEdit {
                workspace_id: session.id,
                path: "file".into(),
                start: 2,
                delete_len: 2,
                replacement: WorkspaceFileReplacement::Inline(b"AB".to_vec()),
            },
            WorkspaceFileRangeEdit {
                workspace_id: session.id,
                path: "file".into(),
                start: 1,
                delete_len: 4,
                replacement: WorkspaceFileReplacement::Inline(b"wxyz".to_vec()),
            },
            WorkspaceFileRangeEdit {
                workspace_id: session.id,
                path: "file".into(),
                start: 1,
                delete_len: 4,
                replacement: WorkspaceFileReplacement::Inline(b"wxyz".to_vec()),
            },
        ])
        .unwrap();
    assert_eq!(std::fs::read(mount.join("file")).unwrap(), b"0wxyz567XY");
    assert!(matches!(
        workspaces.commit_workspace_session(session.id).unwrap(),
        WorkspaceCommitResult::Created { .. }
    ));
    edit(
        &workspaces,
        &session,
        1,
        4,
        WorkspaceFileReplacement::Inline(b"wxyz".to_vec()),
    )
    .unwrap();
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
fn group_2_projection_refresh_commit_and_reopen_are_exact() {
    let (root, workspaces, branch, _) = fixture("projection", b"abcdef");
    let (session, mount) = open_session(&root, &workspaces, branch, "mount");
    assert_eq!(
        workspaces.session(session.id).unwrap().mutation_generation,
        0
    );
    edit(
        &workspaces,
        &session,
        2,
        2,
        WorkspaceFileReplacement::Inline(b"XYZ".to_vec()),
    )
    .unwrap();
    assert_eq!(
        workspaces.session(session.id).unwrap().mutation_generation,
        1
    );
    assert_eq!(std::fs::read(mount.join("file")).unwrap(), b"abXYZef");
    edit(
        &workspaces,
        &session,
        1,
        1,
        WorkspaceFileReplacement::Zero(3),
    )
    .unwrap();
    assert_eq!(
        workspaces.session(session.id).unwrap().mutation_generation,
        2
    );
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
fn group_3_rename_parent_replace_unlink_and_final_alias_reclamation_are_inode_exact() {
    let (root, workspaces, branch, _) = fixture_with("alias-lifecycle", |source| {
        std::fs::create_dir(source.join("parent")).unwrap();
        std::fs::write(source.join("parent/file"), b"abcdef").unwrap();
        std::fs::hard_link(source.join("parent/file"), source.join("parent/alias")).unwrap();
        std::fs::write(source.join("target"), b"replace-me").unwrap();
    });
    let (session, mount) = open_session(&root, &workspaces, branch, "mount");
    std::fs::rename(mount.join("parent"), mount.join("moved")).unwrap();
    std::fs::rename(mount.join("moved/alias"), mount.join("renamed")).unwrap();
    std::fs::rename(mount.join("moved/file"), mount.join("target")).unwrap();
    workspaces
        .edit_workspace_file_range(WorkspaceFileRangeEdit {
            workspace_id: session.id,
            path: "target".into(),
            start: 1,
            delete_len: 3,
            replacement: WorkspaceFileReplacement::Inline(b"X".to_vec()),
        })
        .unwrap();
    assert_eq!(std::fs::read(mount.join("target")).unwrap(), b"aXef");
    assert_eq!(std::fs::read(mount.join("renamed")).unwrap(), b"aXef");
    assert_eq!(
        std::fs::metadata(mount.join("target")).unwrap().ino(),
        std::fs::metadata(mount.join("renamed")).unwrap().ino()
    );
    std::fs::remove_file(mount.join("target")).unwrap();
    workspaces
        .edit_workspace_file_range(WorkspaceFileRangeEdit {
            workspace_id: session.id,
            path: "renamed".into(),
            start: 1,
            delete_len: 1,
            replacement: WorkspaceFileReplacement::Inline(b"YZ".to_vec()),
        })
        .unwrap();
    assert_eq!(std::fs::read(mount.join("renamed")).unwrap(), b"aYZef");
    std::fs::remove_file(mount.join("renamed")).unwrap();
    assert!(matches!(
        workspaces.commit_workspace_session(session.id).unwrap(),
        WorkspaceCommitResult::Created { .. }
    ));
    workspaces
        .end_workspace_session(session.id, EndWorkspaceMode::Clean)
        .unwrap();
    let (reopened, reopen) = open_session(&root, &workspaces, branch, "reopen");
    assert!(!reopen.join("target").exists());
    assert!(!reopen.join("renamed").exists());
    assert!(reopen.join("moved").is_dir());
    workspaces
        .end_workspace_session(reopened.id, EndWorkspaceMode::Clean)
        .unwrap();
    assert!(std::fs::read_dir(root.join("runtime/workspaces"))
        .unwrap()
        .next()
        .is_none());
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
fn group_5_candidate_admission_and_publication_failures_retry_once() {
    let inserted_objects = {
        let (root, workspaces, branch, store) = fixture("commit-boundary-count", b"abcdef");
        let (session, _) = open_session(&root, &workspaces, branch, "mount");
        edit(
            &workspaces,
            &session,
            0,
            0,
            WorkspaceFileReplacement::Inline(b"P".to_vec()),
        )
        .unwrap();
        let before = store.store_counts().unwrap().objects;
        assert!(matches!(
            workspaces.commit_workspace_session(session.id).unwrap(),
            WorkspaceCommitResult::Created { .. }
        ));
        let inserted = store.store_counts().unwrap().objects - before;
        workspaces
            .end_workspace_session(session.id, EndWorkspaceMode::Clean)
            .unwrap();
        std::fs::remove_dir_all(root).unwrap();
        inserted
    };
    for failure in [
        None,
        Some(1_u64),
        Some(inserted_objects + 1),
        Some(inserted_objects + 2),
    ] {
        let label = format!("commit-boundary-{}", failure.unwrap_or(0));
        let (root, workspaces, branch, store) = fixture(&label, b"abcdef");
        let (session, mount) = open_session(&root, &workspaces, branch, "mount");
        edit(
            &workspaces,
            &session,
            0,
            0,
            WorkspaceFileReplacement::Inline(b"P".to_vec()),
        )
        .unwrap();
        let before = store.store_counts().unwrap();
        let branch_before = store.branch(branch).unwrap().unwrap();
        match failure {
            None => inject_candidate_failure_once(),
            Some(statement) => set_transaction_failure_at(Some(statement)),
        }
        let failed = workspaces.commit_workspace_session(session.id);
        set_transaction_failure_at(None);
        assert!(failed.is_err(), "failure={failure:?} result={failed:?}");
        let mut after = store.store_counts().unwrap();
        // Failed staging may retain CAS rows, but must not publish any metadata.
        assert!(after.objects >= before.objects);
        assert!(after.objects <= before.objects + inserted_objects);
        if failure.is_none() {
            assert_eq!(after.objects, before.objects);
        }
        after.objects = before.objects;
        assert_eq!(after, before);
        assert_eq!(store.branch(branch).unwrap().unwrap(), branch_before);
        assert_eq!(std::fs::read(mount.join("file")).unwrap(), b"Pabcdef");
        assert!(matches!(
            workspaces.commit_workspace_session(session.id).unwrap(),
            WorkspaceCommitResult::Created { .. }
        ));
        assert_eq!(store.store_counts().unwrap().commits, before.commits + 1);
        assert_eq!(
            store.store_counts().unwrap().objects,
            before.objects + inserted_objects
        );
        assert!(matches!(
            workspaces.commit_workspace_session(session.id).unwrap(),
            WorkspaceCommitResult::UpToDate { .. }
        ));
        workspaces
            .end_workspace_session(session.id, EndWorkspaceMode::Clean)
            .unwrap();
        std::fs::remove_dir_all(root).unwrap();
    }
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

#[test]
fn stale_head_with_resume_failure_requires_explicit_presentation_recovery() {
    let (root, workspaces, branch, store) = fixture("stale-resume", b"abcdef");
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
    inject_projection_resume_failure_once();
    assert!(matches!(
        workspaces.commit_workspace_session(stale.id),
        Err(WorkspaceError::Io(_))
    ));
    assert_eq!(
        workspaces.session(stale.id).unwrap().session.state,
        WorkspaceState::Active
    );
    assert_eq!(
        workspaces
            .recover_workspace_presentation(stale.id)
            .unwrap()
            .state,
        WorkspaceState::Active
    );
    workspaces
        .end_workspace_session(stale.id, EndWorkspaceMode::Discard)
        .unwrap();
    std::fs::remove_dir_all(root).unwrap();
}
