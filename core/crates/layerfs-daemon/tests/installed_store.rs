//! Direct ports and multiple Workspace bindings over one installed global Store.
#[path = "support/held_writer.rs"]
mod held_writer;
#[path = "support/installed_store.rs"]
mod support;
use layerfs_content::filesystem::PathName;
use layerfs_content::{AuthenticatedObjects, ContentError, ObjectId};
use layerfs_daemon::{
    bootstrap::open_store,
    store::{BindError, BindPhase, BindRequest, BoundWorkspace, PortError, Store},
    Command, Completion, Owner, OwnerClient, OwnerConfig, Response,
};
use layerfs_history::{BranchId, WorkspaceId};
use layerfs_overlay::{ProfileConfig, Route};
use layerfs_storage::StorageError;
use layerfs_workspace::InodeSerials;
use std::{
    collections::BTreeSet,
    sync::{mpsc, Arc},
    time::{Duration, Instant},
};

fn job(client: &OwnerClient, route: Route, command: Command) -> Completion {
    let pending = client
        .try_submit(Some(route), command)
        .unwrap_or_else(|e| panic!("{e:?}"));
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        if let Some(done) = pending.try_complete().unwrap() {
            return done;
        }
        assert!(Instant::now() < deadline, "bounded original completion");
        std::thread::yield_now();
    }
}
fn bind(store: &Arc<Store>, owner: &Owner, fixture: &support::Fixture, tag: u8) -> BoundWorkspace {
    let attached = store
        .bind(
            owner.client(),
            BindRequest {
                branch: fixture.branch,
                workspace: WorkspaceId::from_authority([tag; 32]).unwrap(),
            },
        )
        .unwrap();
    assert!(matches!(attached.open.result(), Ok(Response::Opened(_))));
    assert_eq!(
        attached
            .open
            .work()
            .sql
            .family(layerfs_overlay::StatementKind::Begin)
            .unwrap()
            .executions,
        1
    );
    attached.workspace
}
fn read(workspace: &BoundWorkspace, n: usize) {
    let operation = workspace.operation().unwrap();
    let base = operation.workspace().base().unwrap();
    let file = base
        .child(
            base.root().root_inode().serial(),
            &PathName::new(&format!("file-{n:06}")).unwrap(),
        )
        .unwrap();
    let mut actual = Vec::new();
    base.plan_read(file.serial, 0, 1024)
        .unwrap()
        .emit(&mut actual)
        .unwrap();
    assert_eq!(actual, support::bytes(n));
    assert_eq!(
        base.stat(file.serial).unwrap().logical_len,
        actual.len() as u64
    );
    assert!(operation.ports().failure().unwrap().is_none());
}

#[test]
fn bounded_bind_batches_lengths_cache_and_original_failure() {
    for count in [1, 1024] {
        let fixture = support::Fixture::new(count, &format!("bind-{count}"));
        let store = open_store(
            fixture.config.clone(),
            support::BINDING,
            support::CURSOR,
            2,
            2 * 1024 * 1024,
            layerfs_storage::ReservationBlocks::default(),
        )
        .unwrap();
        let owner = Owner::start(
            &fixture.directory.join("overlay.sqlite"),
            ProfileConfig::default(),
            OwnerConfig::default(),
        )
        .unwrap();
        let before = store.work();
        let first = bind(&store, &owner, &fixture, 1);
        let cold = store.work().object_batches - before.object_batches;
        assert!(
            (4..=16).contains(&cold),
            "bounded root paths, count={count}, batches={cold}"
        );
        let before = store.work();
        let second = bind(&store, &owner, &fixture, 2);
        assert_eq!(
            store.work(),
            before,
            "shared warm root has zero Store demands"
        );
        assert_ne!(first.route(), second.route());
        assert_eq!(
            first.snapshot().effective_root,
            second.snapshot().effective_root
        );
        read(&first, count - 1);
        let operation = second.operation().unwrap();
        let base = operation.workspace().base().unwrap();
        let file = base
            .child(
                base.root().root_inode().serial(),
                &PathName::new(&format!("file-{:06}", count - 1)).unwrap(),
            )
            .unwrap();
        let ids = [
            base.identity().0,
            base.root().inode_table(),
            file.value.content_root,
        ];
        let before = store.work();
        let values = operation.ports().read_canonical_batch(&ids).unwrap();
        assert_eq!(store.work().object_batches - before.object_batches, 1);
        assert_eq!(store.work().object_ids - before.object_ids, 3);
        assert_eq!(
            values
                .iter()
                .map(|bytes| ObjectId::for_bytes(bytes))
                .collect::<Vec<_>>(),
            ids
        );
        let before = store.work();
        let lengths = operation
            .ports()
            .file_lengths(&[file.value.content_root, file.value.content_root])
            .unwrap();
        assert_eq!(lengths, vec![support::bytes(count - 1).len() as u64; 2]);
        assert_eq!(store.work().length_batches - before.length_batches, 1);
        operation
            .client()
            .read_canonical(file.value.content_root)
            .unwrap();
        let before = store.work();
        operation
            .client()
            .read_canonical(file.value.content_root)
            .unwrap();
        assert_eq!(store.work(), before, "warm object causes zero Store calls");

        let failed = first.operation().unwrap();
        let missing = ObjectId::for_bytes(b"absent installed object");
        assert_eq!(
            failed.ports().read_canonical(missing),
            Err(ContentError::MissingObject)
        );
        let original = failed.ports().failure().unwrap().unwrap();
        assert!(
            matches!(original.as_ref(), PortError::Storage(StorageError::ObjectMissing(id)) if *id == missing)
        );
        let before = store.work();
        assert_eq!(
            failed.ports().read_canonical(ids[0]),
            Err(ContentError::MissingObject)
        );
        assert_eq!(
            store.work(),
            before,
            "failed operation does not issue another demand"
        );
        assert!(Arc::ptr_eq(
            &original,
            &failed.ports().failure().unwrap().unwrap()
        ));
        read(&second, 0);

        let before = owner.client().diagnostics().unwrap().admitted;
        let refused = store
            .bind(
                owner.client(),
                BindRequest {
                    branch: BranchId::from_authority([99; 16]),
                    workspace: WorkspaceId::from_authority([3; 32]).unwrap(),
                },
            )
            .err()
            .unwrap();
        assert_eq!(refused.phase, BindPhase::Snapshot);
        assert!(matches!(refused.error, BindError::MissingBranch(_)));
        assert_eq!(owner.client().diagnostics().unwrap().admitted, before);
        let duplicate = store
            .bind(
                owner.client(),
                BindRequest {
                    branch: fixture.branch,
                    workspace: first.identity(),
                },
            )
            .err()
            .unwrap();
        assert_eq!(duplicate.phase, BindPhase::Open);
        assert!(matches!(duplicate.error, BindError::Completion(_)));
        drop(duplicate);
        read(&first, 0);
        println!("DIRECT_STORE count={} cold_bind_batches={cold} warm_bind_batches=0 fixed_read_handles={} adapter={:?} cache={:?}", fixture.count, store.read_handles(), store.work(), store.cache_work().unwrap());
        drop((failed, operation, base));
        for workspace in [&first, &second] {
            assert!(matches!(
                job(&owner.client(), workspace.route(), Command::Close).result(),
                Ok(Response::Done)
            ));
        }
        drop((first, second, store));
        owner.stop().unwrap();
        fixture.cleanup();
    }
}

#[test]
fn four_workspaces_read_concurrently_and_consume_disjoint_scope_ranges() {
    let fixture = support::Fixture::new(32, "concurrent");
    let store = open_store(
        fixture.config.clone(),
        support::BINDING,
        support::CURSOR,
        3,
        2 * 1024 * 1024,
        layerfs_storage::ReservationBlocks::default(),
    )
    .unwrap();
    let owner = Owner::start(
        &fixture.directory.join("overlay.sqlite"),
        ProfileConfig::default(),
        OwnerConfig::default(),
    )
    .unwrap();
    let workspaces: Vec<_> = (1..=4)
        .map(|tag| Arc::new(bind(&store, &owner, &fixture, tag)))
        .collect();
    // Each original reservation is explicit and sequential. Concurrent readers
    // consume these independent local ranges; no write refusal is retried.
    let starts: Vec<_> = workspaces
        .iter()
        .map(|workspace| {
            let operation = workspace.operation().unwrap();
            operation
                .workspace()
                .next_serial(operation.ports())
                .unwrap()
        })
        .collect();
    let (tx, rx) = mpsc::channel();
    let workers: Vec<_> = workspaces
        .iter()
        .enumerate()
        .map(|(index, workspace)| {
            let workspace = workspace.clone();
            let tx = tx.clone();
            let first = starts[index];
            std::thread::spawn(move || {
                let operation = workspace.operation().unwrap();
                let mut serials = vec![first];
                for n in 0..8 {
                    read(&workspace, index * 8 + n);
                    if n != 0 {
                        serials.push(
                            operation
                                .workspace()
                                .next_serial(operation.ports())
                                .unwrap(),
                        );
                    }
                }
                tx.send(serials).unwrap();
            })
        })
        .collect();
    drop(tx);
    let mut serials = BTreeSet::new();
    for _ in 0..4 {
        for serial in rx.recv_timeout(Duration::from_secs(5)).unwrap() {
            assert!(serials.insert(serial));
        }
    }
    for worker in workers {
        worker.join().unwrap();
    }
    assert_eq!(serials.len(), 32);
    assert_eq!(store.work().serial_reservations, 4);
    assert_eq!(owner.client().diagnostics().unwrap().outstanding, 0);
    println!("DIRECT_STORE_MULTI workspaces=4 callers=4 distinct_serials=32 reservations=4 overlay_databases=1 read_handles=3 work={:?}", store.work());
    for workspace in &workspaces {
        assert!(matches!(
            job(&owner.client(), workspace.route(), Command::Close).result(),
            Ok(Response::Done)
        ));
    }
    drop((workspaces, store));
    owner.stop().unwrap();
    fixture.cleanup();
}

#[test]
fn direct_reads_finish_under_another_process_writer_and_busy_stays_typed() {
    let fixture = support::Fixture::new(2, "held-writer");
    let store = open_store(
        fixture.config.clone(),
        support::BINDING,
        support::CURSOR,
        2,
        1024 * 1024,
        layerfs_storage::ReservationBlocks::default(),
    )
    .unwrap();
    let owner = Owner::start(
        &fixture.directory.join("overlay.sqlite"),
        ProfileConfig::default(),
        OwnerConfig::default(),
    )
    .unwrap();
    let workspace = bind(&store, &owner, &fixture, 1);
    let before = workspace.operation().unwrap().ports().reserve(1).unwrap().0;
    let held = held_writer::HeldWriter::acquire(&fixture.config.path);
    let operation = workspace.operation().unwrap();
    let root = workspace.snapshot().effective_root;
    assert_eq!(
        ObjectId::for_bytes(&operation.ports().read_canonical(root).unwrap()),
        root
    );
    assert!(operation.ports().reserve(1).is_err());
    assert!(matches!(
        operation.ports().failure().unwrap().unwrap().as_ref(),
        PortError::History(layerfs_history::HistoryError::Busy)
    ));
    let independent = workspace.operation().unwrap();
    assert_eq!(
        ObjectId::for_bytes(&independent.ports().read_canonical(root).unwrap()),
        root
    );
    held.release();
    let later = workspace.operation().unwrap();
    assert_eq!(later.ports().reserve(1).unwrap().0, before + 1);
    println!("DIRECT_STORE_WRITER reads_during_hold=2 busy=History::Busy next_explicit_reservation_gap=0 child=released-and-joined");
    assert!(matches!(
        job(&owner.client(), workspace.route(), Command::Close).result(),
        Ok(Response::Done)
    ));
    drop((operation, independent, later, workspace, store));
    owner.stop().unwrap();
    fixture.cleanup();
}
