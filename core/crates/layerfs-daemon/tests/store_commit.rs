//! Store-half Commit over real saved Content and the real local owner.
#[path = "support/held_writer.rs"]
mod held_writer;
#[path = "support/history_boundary.rs"]
mod history_boundary;
#[path = "support/installed_store.rs"]
mod support;
use layerfs_content::filesystem::{
    FilesystemInput, FilesystemObjects, FilesystemRead, FilesystemResources, FilesystemRoot,
    FilesystemRootId, InodeScope, InodeUpdate, PathName,
};
use layerfs_content::{
    AuthenticatedObjects, ConstructionPolicy, FinalizedObject, ObjectId, ObjectRole,
};
use layerfs_daemon::{
    bootstrap::open_store,
    store::{BindRequest, BoundWorkspace, CommitError, CommitPhase, Store},
    Command, Completion, Owner, OwnerClient, OwnerConfig, Response,
};
use layerfs_history::{BranchSnapshot, CommitStagedOutcome, HistoryError, WorkspaceId};
use layerfs_overlay::{ProfileConfig, Route};
use layerfs_storage::{ReservationBlocks, Save, StorageError};
use layerfs_telemetry::timer::Timing;
use layerfs_workspace::{Operation, Outcome, Position, Time};
use std::{
    cell::Cell,
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc, Arc,
    },
    time::{Duration, Instant},
};

fn job(owner: &OwnerClient, route: Route, command: Command) -> Completion {
    let pending = owner
        .try_submit(Some(route), command)
        .unwrap_or_else(|e| panic!("{e:?}"));
    let until = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(done) = pending.try_complete().unwrap() {
            return done;
        }
        assert!(Instant::now() < until, "bounded original local completion");
        std::thread::yield_now();
    }
}
fn bind(store: &Arc<Store>, owner: &Owner, f: &support::Fixture, tag: u8) -> Arc<BoundWorkspace> {
    Arc::new(
        store
            .bind(
                owner.client(),
                BindRequest {
                    branch: f.branch,
                    workspace: WorkspaceId::from_authority([tag; 32]).unwrap(),
                },
            )
            .unwrap()
            .workspace,
    )
}
fn setup(label: &str) -> (support::Fixture, Arc<Store>, Owner) {
    let f = support::Fixture::new(8, label);
    println!("STORE_COMMIT_FIXTURE case={label} files={}", f.count);
    let store = open_store(
        f.config.clone(),
        support::BINDING,
        support::CURSOR,
        2,
        2 * 1024 * 1024,
        ReservationBlocks::default(),
    )
    .unwrap();
    let owner = Owner::start(
        &f.directory.join("overlay.sqlite"),
        ProfileConfig::default(),
        OwnerConfig::default(),
    )
    .unwrap();
    (f, store, owner)
}
fn bytes(tag: u8) -> Vec<u8> {
    let mut bytes = support::bytes(0);
    bytes[0] = tag;
    bytes
}
fn window<T>(
    workspace: &BoundWorkspace,
    f: impl FnOnce(&layerfs_daemon::store::StoreOperation, &layerfs_workspace::SourceView) -> T,
) -> T {
    static NEXT: AtomicU64 = AtomicU64::new(100);
    let operation = workspace.operation().unwrap();
    let acquired = job(
        operation.overlay(),
        workspace.route(),
        Command::AcquireBaseSource {
            owner: NEXT.fetch_add(1, Ordering::Relaxed),
        },
    );
    let source = match acquired.result() {
        Ok(Response::BaseSource(source)) => *source,
        other => panic!("{other:?}"),
    };
    drop(acquired);
    let view = operation.workspace().view_for_source(source).unwrap();
    let value = f(&operation, &view);
    drop(view);
    assert!(job(
        operation.overlay(),
        workspace.route(),
        Command::ReleaseBaseSource(source)
    )
    .result()
    .is_ok());
    value
}
fn write(workspace: &BoundWorkspace, value: &[u8]) {
    window(workspace, |operation, view| {
        let stat = view
            .lookup(
                operation.overlay(),
                view.root_serial(),
                &PathName::new("file-000000").unwrap(),
            )
            .unwrap();
        assert_eq!(stat.logical_len, value.len() as u64);
        let outcome = operation
            .workspace()
            .mutate(
                operation.overlay(),
                operation.ports(),
                view,
                Operation::Write {
                    serial: stat.serial,
                    position: Position::At(0),
                    data: value.into(),
                },
                Time {
                    seconds: stat.metadata.mtime_seconds,
                    nanoseconds: stat.metadata.mtime_nanoseconds,
                },
            )
            .unwrap();
        let publication = match outcome {
            Outcome::Applied { publication, .. } => publication,
            other => panic!("{other:?}"),
        };
        assert!(job(
            operation.overlay(),
            workspace.route(),
            Command::ReplyAttempted(publication)
        )
        .result()
        .is_ok());
    });
}
fn read(workspace: &BoundWorkspace) -> Vec<u8> {
    window(workspace, |operation, view| {
        let stat = view
            .lookup(
                operation.overlay(),
                view.root_serial(),
                &PathName::new("file-000000").unwrap(),
            )
            .unwrap();
        let mut bytes = Vec::new();
        view.read(operation.overlay(), stat.serial, 0, 1024, &mut bytes)
            .unwrap();
        bytes
    })
}
fn construct(
    save: &Save<'_>,
    snapshot: &BranchSnapshot,
    value: &[u8],
    policy: ConstructionPolicy,
) -> Result<FilesystemRootId, CommitError> {
    let mut reader = FilesystemRead::new(save, FilesystemRootId(snapshot.effective_root))?;
    let root = reader.root();
    let mut file =
        reader.resolve_child(root.root_inode().serial(), &PathName::new("file-000000")?)?;
    let mut sink = save.sink();
    let built = Timing::disabled("commit.file", |scope| {
        layerfs_content::construct_bytes(
            policy,
            &policy.capacities(),
            value,
            &mut sink,
            scope.child("construct"),
        )
    })
    .0?;
    file.value.content_root = built.root;
    let input = FilesystemInput {
        base: Some(FilesystemRootId(snapshot.effective_root)),
        scope: root.scope(),
        root_serial: root.root_inode().serial(),
        directories: &[],
        inodes: &[InodeUpdate {
            serial: file.serial,
            value: file.value,
        }],
        new_inodes: &[],
        resources: FilesystemResources::default(),
    };
    let mut objects = FilesystemObjects::new_with_accepted(save, &mut sink, save);
    Ok(layerfs_content::filesystem::update_filesystem(&mut objects, &input, None)?.root)
}
fn close(owner: &Owner, workspace: &BoundWorkspace) {
    assert!(matches!(
        job(&owner.client(), workspace.route(), Command::Close).result(),
        Ok(Response::Done)
    ));
}

#[test]
fn committed_up_to_date_conflict_and_new_bind_preserve_exact_roots() {
    let (f, store, owner) = setup("commit-outcomes");
    let first = bind(&store, &owner, &f, 1);
    let loser = bind(&store, &owner, &f, 2);
    let original = first.snapshot().unwrap();
    write(&first, &bytes(b'A'));
    write(&loser, &bytes(b'B'));
    let result = first
        .commit(|save, _, snapshot| {
            construct(save, snapshot, &bytes(b'A'), store.policy().construction())
        })
        .unwrap();
    let record = match &result.history {
        CommitStagedOutcome::Committed(record) => record.clone(),
        other => panic!("{other:?}"),
    };
    assert_eq!(result.storage.initial_reservations, 1);
    assert_eq!(result.storage.reservation_refills, 0);
    assert_eq!(result.storage.reserve, 1);
    assert!(result.storage.publish > 0);
    assert_eq!(read(&first), bytes(b'A'));
    assert_eq!(
        first.snapshot().unwrap().branch.head_commit,
        Some(record.id)
    );
    assert!(!first.commit_in_flight());
    assert!(store.history().stage(first.identity()).unwrap().is_none());
    println!(
        "STORE_COMMIT count={} initial={} refills={} publication={} history=1 installed=true",
        result.storage.reserve + result.storage.publish + 1,
        result.storage.initial_reservations,
        result.storage.reservation_refills,
        result.storage.publish
    );
    drop(result);
    let same = first
        .commit(|_, _, snapshot| Ok(FilesystemRootId(snapshot.effective_root)))
        .unwrap();
    assert_eq!(
        same.history,
        CommitStagedOutcome::UpToDate {
            head: Some(record.id),
            root: record.root
        }
    );
    println!(
        "STORE_UP_TO_DATE count={} initial={} refills={} publication={} history=1",
        same.storage.reserve + same.storage.publish + 1,
        same.storage.initial_reservations,
        same.storage.reservation_refills,
        same.storage.publish
    );
    drop(same);
    let conflict = loser
        .commit(|save, _, snapshot| {
            construct(save, snapshot, &bytes(b'B'), store.policy().construction())
        })
        .unwrap_err();
    assert_eq!(conflict.phase, CommitPhase::Publish);
    assert!(
        matches!(&conflict.error, CommitError::History(HistoryError::HeadMoved(m)) if m.expected_head == original.branch.head_commit && m.actual_head == Some(record.id) && m.expected_base == original.branch.base_layer && m.actual_base == original.branch.base_layer)
    );
    assert!(conflict.locally_settled);
    assert!(conflict.published.is_none());
    assert!(store.history().stage(loser.identity()).unwrap().is_none());
    assert_eq!(loser.snapshot().unwrap(), original);
    assert_eq!(read(&loser), bytes(b'B'));
    drop(conflict);
    let rebound = bind(&store, &owner, &f, 3);
    assert_eq!(rebound.snapshot().unwrap().effective_root, record.root);
    assert_eq!(read(&rebound), bytes(b'A'));
    for workspace in [&first, &loser, &rebound] {
        close(&owner, workspace);
    }
    drop((first, loser, rebound, store));
    owner.stop().unwrap();
    f.cleanup();
}

#[test]
fn busy_missing_dependency_and_read_failure_leave_no_stage_or_local_data_loss() {
    let (f, store, owner) = setup("commit-refusals");
    let workspace = bind(&store, &owner, &f, 1);
    let original = workspace.snapshot().unwrap();
    write(&workspace, &bytes(b'C'));
    let held = held_writer::HeldWriter::acquire(&f.config.path);
    let called = Cell::new(false);
    let busy = workspace
        .commit(|_, _, snapshot| {
            called.set(true);
            Ok(FilesystemRootId(snapshot.effective_root))
        })
        .unwrap_err();
    assert!(!called.get());
    assert!(matches!(
        busy.error,
        CommitError::Storage(StorageError::Busy)
    ));
    assert_eq!(busy.phase, CommitPhase::Begin);
    assert!(busy.locally_settled && busy.published.is_none());
    assert!(store
        .history()
        .stage(workspace.identity())
        .unwrap()
        .is_none());
    assert_eq!(read(&workspace), bytes(b'C'));
    assert_eq!(workspace.snapshot().unwrap(), original);
    held.release();
    drop(busy);
    let absent = ObjectId::for_bytes(b"absent inode dependency");
    let missing = workspace
        .commit(|save, _, snapshot| {
            let root = FilesystemRoot::new(
                snapshot.profile,
                InodeScope::from_object(snapshot.scope),
                1,
                absent,
            )?;
            let object = FinalizedObject::new(ObjectRole::FilesystemRoot, root.encode()?)?
                .with_references(vec![absent]);
            let id = object.id();
            save.accept(object)?;
            Ok(FilesystemRootId(id))
        })
        .unwrap_err();
    assert!(
        matches!(&missing.error, CommitError::Storage(StorageError::MissingDependency { reference, .. }) if *reference == absent),
        "{missing:?}"
    );
    assert!(missing.locally_settled && missing.published.is_none());
    assert!(store
        .history()
        .stage(workspace.identity())
        .unwrap()
        .is_none());
    drop(missing);
    let missing_read = workspace
        .commit(|save, _, _| {
            save.read_canonical(absent)?;
            unreachable!()
        })
        .unwrap_err();
    assert!(
        matches!(&missing_read.error, CommitError::Construction { storage: StorageError::ObjectMissing(id), .. } if *id == absent)
    );
    assert!(missing_read.locally_settled);
    assert_eq!(read(&workspace), bytes(b'C'));
    drop(missing_read);
    let later = workspace
        .commit(|save, _, snapshot| {
            construct(save, snapshot, &bytes(b'C'), store.policy().construction())
        })
        .unwrap();
    assert!(matches!(later.history, CommitStagedOutcome::Committed(_)));
    assert_eq!(read(&workspace), bytes(b'C'));
    println!("STORE_COMMIT_REFUSALS busy_before_construction=true no_new_stage=true effective_bytes_preserved=true original_missing_dependency=true original_read_failure=true later_explicit_call=Committed");
    drop(later);
    close(&owner, &workspace);
    drop((workspace, store));
    owner.stop().unwrap();
    f.cleanup();
}

#[test]
fn known_publication_survives_an_unattempted_local_install() {
    let (f, store, owner) = setup("commit-install-refusal");
    let workspace = bind(&store, &owner, &f, 1);
    let original = workspace.snapshot().unwrap();
    write(&workspace, &bytes(b'D'));
    let mut owner = Some(owner);
    let failed = workspace
        .commit(|save, _, snapshot| {
            let root = construct(save, snapshot, &bytes(b'D'), store.policy().construction())?;
            owner.take().unwrap().stop().unwrap();
            Ok(root)
        })
        .unwrap_err();
    assert_eq!(failed.phase, CommitPhase::Install);
    assert!(matches!(
        &failed.error,
        CommitError::Owner(layerfs_daemon::OwnerError::Unattempted { .. })
    ));
    assert!(matches!(
        failed.published,
        Some(CommitStagedOutcome::Committed(_))
    ));
    assert!(!failed.locally_settled && workspace.commit_in_flight());
    assert_eq!(workspace.snapshot().unwrap(), original);
    assert!(failed.capture.is_some() && failed.captured.is_some());
    assert!(matches!(
        workspace
            .commit(|_, _, _| unreachable!())
            .unwrap_err()
            .error,
        CommitError::InFlight
    ));
    let next_owner = Owner::start(
        &f.directory.join("next-overlay.sqlite"),
        ProfileConfig::default(),
        OwnerConfig::default(),
    )
    .unwrap();
    let next = bind(&store, &next_owner, &f, 2);
    assert_eq!(read(&next), bytes(b'D'));
    println!("STORE_COMMIT_INSTALL_FAILURE history=known-Committed install=unattempted original_capture=retained next_commit=InFlight new_workspace=published-bytes");
    close(&next_owner, &next);
    drop((failed, next, workspace, store));
    next_owner.stop().unwrap();
    f.cleanup();
}

#[test]
fn a_held_producer_does_not_hold_workspace_metadata_or_read_capacity() {
    let (f, store, owner) = setup("commit-admission");
    let workspace = bind(&store, &owner, &f, 1);
    let (ready, entered) = mpsc::channel();
    let (release, proceed) = mpsc::channel();
    let active = workspace.clone();
    let worker = std::thread::spawn(move || {
        active.commit(|_, _, snapshot| {
            ready
                .send(())
                .map_err(|_| CommitError::Context("peer ended"))?;
            proceed
                .recv_timeout(Duration::from_secs(3))
                .map_err(|_| CommitError::Context("bounded producer hold"))?;
            Ok(FilesystemRootId(snapshot.effective_root))
        })
    });
    entered.recv_timeout(Duration::from_secs(3)).unwrap();
    assert!(matches!(
        workspace
            .commit(|_, _, _| unreachable!())
            .unwrap_err()
            .error,
        CommitError::InFlight
    ));
    assert_eq!(read(&workspace), support::bytes(0));
    release.send(()).unwrap();
    let done = worker.join().unwrap().unwrap();
    assert!(matches!(done.history, CommitStagedOutcome::UpToDate { .. }));
    drop(done);
    close(&owner, &workspace);
    drop((workspace, store));
    owner.stop().unwrap();
    f.cleanup();
}

#[test]
fn later_mutation_survives_capture_publication_and_known_install() {
    let (f, store, owner) = setup("commit-later-mutation");
    let workspace = bind(&store, &owner, &f, 1);
    write(&workspace, &bytes(b'G'));
    let first = workspace
        .commit(|save, _, snapshot| {
            let root = construct(save, snapshot, &bytes(b'G'), store.policy().construction())?;
            write(&workspace, &bytes(b'H'));
            Ok(root)
        })
        .unwrap();
    let first_record = match first.history {
        CommitStagedOutcome::Committed(ref record) => record.clone(),
        ref other => panic!("{other:?}"),
    };
    assert_eq!(read(&workspace), bytes(b'H'));
    let old_view = bind(&store, &owner, &f, 2);
    assert_eq!(read(&old_view), bytes(b'G'));
    drop(first);
    let second = workspace
        .commit(|save, _, snapshot| {
            construct(save, snapshot, &bytes(b'H'), store.policy().construction())
        })
        .unwrap();
    assert!(
        matches!(&second.history, CommitStagedOutcome::Committed(record) if record.parent == Some(first_record.id))
    );
    assert_eq!(read(&workspace), bytes(b'H'));
    assert_eq!(
        read(&old_view),
        bytes(b'G'),
        "an existing Workspace retains its own base"
    );
    let latest = bind(&store, &owner, &f, 3);
    assert_eq!(read(&latest), bytes(b'H'));
    let op = latest.operation().unwrap();
    let base = op.workspace().base().unwrap();
    assert_eq!(
        base.list(base.root().root_inode().serial(), None, 64, 65536)
            .unwrap()
            .entries
            .len(),
        f.count
    );
    println!("STORE_LATER_MUTATION captured=G first_published=G live_after_install=H next_published=H old_workspace=G namespace_entries=8");
    drop((second, op, base));
    for view in [&workspace, &old_view, &latest] {
        close(&owner, view);
    }
    drop((workspace, old_view, latest, store));
    owner.stop().unwrap();
    f.cleanup();
}

fn observed_store(
    f: &support::Fixture,
    boundary: history_boundary::Boundary,
) -> (Arc<Store>, Arc<layerfs_persistence::Handles>) {
    use layerfs_persistence::Handles;
    let handles = Arc::new(
        Handles::open_writable(f.config.clone(), support::BINDING, support::CURSOR).unwrap(),
    );
    let readers = (0..2)
        .map(|_| {
            let read = Handles::open_read_only(f.config.clone(), support::BINDING, support::CURSOR)
                .unwrap();
            layerfs_storage::Storage::new(read.storage).unwrap()
        })
        .collect();
    let history = history_boundary::ObservedHistory {
        handles: handles.clone(),
        database: f.config.path.clone(),
        boundary,
        once: std::sync::atomic::AtomicBool::new(true),
    };
    let store = Store::new(
        handles.storage.clone(),
        Arc::new(history),
        readers,
        2 * 1024 * 1024,
        ReservationBlocks::default(),
    )
    .unwrap();
    (Arc::new(store), handles)
}

#[test]
fn history_busy_after_save_has_no_stage_and_counts_only_acknowledged_save_writes() {
    use layerfs_history::HistoryCatalog;
    let f = support::Fixture::new(8, "commit-history-busy");
    let (store, handles) = observed_store(&f, history_boundary::Boundary::HeldWriter);
    let owner = Owner::start(
        &f.directory.join("overlay.sqlite"),
        ProfileConfig::default(),
        OwnerConfig::default(),
    )
    .unwrap();
    let workspace = bind(&store, &owner, &f, 1);
    let original = workspace.snapshot().unwrap();
    write(&workspace, &bytes(b'E'));
    let before = handles.diagnostics().unwrap();
    let failed = workspace
        .commit(|save, _, snapshot| {
            construct(save, snapshot, &bytes(b'E'), store.policy().construction())
        })
        .unwrap_err();
    assert_eq!(failed.phase, CommitPhase::Publish);
    assert!(matches!(
        failed.error,
        CommitError::History(HistoryError::Busy)
    ));
    assert!(failed.saved.is_some() && failed.prepared.is_some());
    assert!(failed.locally_settled && failed.published.is_none());
    let counts = failed.storage.unwrap();
    let after = handles.diagnostics().unwrap();
    assert_eq!(
        after.write_commits - before.write_commits,
        counts.reserve + counts.publish
    );
    assert_eq!(
        after.write_transactions - before.write_transactions,
        counts.reserve + counts.publish
    );
    assert!(handles
        .history
        .stage(workspace.identity())
        .unwrap()
        .is_none());
    assert_eq!(workspace.snapshot().unwrap(), original);
    assert_eq!(read(&workspace), bytes(b'E'));
    drop(failed);
    let before = handles.diagnostics().unwrap();
    let later = workspace
        .commit(|save, _, snapshot| {
            construct(save, snapshot, &bytes(b'E'), store.policy().construction())
        })
        .unwrap();
    let actual = handles.diagnostics().unwrap().write_transactions - before.write_transactions;
    assert_eq!(actual, later.storage.reserve + later.storage.publish + 1);
    println!("STORE_HISTORY_BUSY save_writes={} failed_history_statements=1 new_stage_rows=0 local_bytes=preserved later_explicit_call_writes={actual}", counts.reserve + counts.publish);
    drop(later);
    close(&owner, &workspace);
    drop((workspace, store, handles));
    owner.stop().unwrap();
    f.cleanup();
}

#[test]
fn unknown_history_acknowledgement_retains_original_intent_and_capture() {
    use layerfs_history::HistoryCatalog;
    let f = support::Fixture::new(8, "commit-unknown-ack");
    let (store, handles) = observed_store(&f, history_boundary::Boundary::LostAcknowledgement);
    let owner = Owner::start(
        &f.directory.join("overlay.sqlite"),
        ProfileConfig::default(),
        OwnerConfig::default(),
    )
    .unwrap();
    let workspace = bind(&store, &owner, &f, 1);
    let original = workspace.snapshot().unwrap();
    write(&workspace, &bytes(b'F'));
    let failed = workspace
        .commit(|save, _, snapshot| {
            construct(save, snapshot, &bytes(b'F'), store.policy().construction())
        })
        .unwrap_err();
    assert_eq!(failed.phase, CommitPhase::Publish);
    assert!(matches!(
        failed.error,
        CommitError::History(HistoryError::UnknownOutcome)
    ));
    assert!(
        failed.capture.is_some()
            && failed.captured.is_some()
            && failed.intent.is_some()
            && failed.prepared.is_some()
    );
    assert!(failed.published.is_none() && failed.local.is_none() && failed.local_error.is_none());
    assert!(!failed.locally_settled && workspace.commit_in_flight());
    assert_eq!(workspace.snapshot().unwrap(), original);
    assert!(matches!(
        workspace
            .commit(|_, _, _| unreachable!())
            .unwrap_err()
            .error,
        CommitError::InFlight
    ));
    let observer = layerfs_persistence::Handles::open_read_only(
        f.config.clone(),
        support::BINDING,
        support::CURSOR,
    )
    .unwrap();
    let observed = observer.history.branch_snapshot(f.branch).unwrap().unwrap();
    assert_eq!(
        observed.effective_root,
        failed.intent.as_ref().unwrap().candidate_root
    );
    assert!(
        workspace.commit_in_flight(),
        "an observer read does not settle the original unknown"
    );
    println!("STORE_UNKNOWN_ACK original=UnknownOutcome capture_and_intent=retained local_resolution=not-attempted install=not-attempted observer_does_not_resolve=true fixture_teardown=explicit-after-owner-stop");
    owner.stop().unwrap();
    drop((observer, failed, workspace, store, handles));
    f.cleanup();
}
