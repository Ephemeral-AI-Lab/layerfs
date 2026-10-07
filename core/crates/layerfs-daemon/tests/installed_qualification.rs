//! Explicit topology qualification through installed Store ports and real local records.
#[path = "support/installed_store.rs"]
mod copied;
#[allow(dead_code)]
#[path = "support/control_fixture.rs"]
mod native;
#[allow(dead_code)]
#[path = "support/native_install.rs"]
mod support;
use layerfs_content::filesystem::{qualify_root, QualificationWork, RootContext};
use layerfs_content::{ContentError, ObjectId};
use layerfs_daemon::{
    bootstrap::open_store,
    store::{BindRequest, BoundWorkspace, Store},
    Command, Completion, Owner, OwnerClient, OwnerConfig, Response,
};
use layerfs_history::{BranchId, WorkspaceId};
use layerfs_overlay::{IndexedOperationRecordScope, ProfileConfig, Route};
use layerfs_workspace::IndexedEditRecords;
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

fn job(client: &OwnerClient, route: Route, command: Command) -> Completion {
    let pending = client.try_submit(Some(route), command).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(done) = pending.try_complete().unwrap() {
            return done;
        }
        assert!(Instant::now() < deadline, "bounded original result");
        std::thread::yield_now();
    }
}
fn bind(store: &Arc<Store>, owner: &Owner, branch: BranchId, tag: u8) -> BoundWorkspace {
    let before = owner.client().diagnostics().unwrap().admitted;
    let bound = store
        .bind(
            owner.client(),
            BindRequest {
                branch,
                workspace: WorkspaceId::from_authority([tag; 32]).unwrap(),
            },
        )
        .unwrap();
    assert_eq!(
        owner.client().diagnostics().unwrap().admitted - before,
        1,
        "bind has one Open, no operation acquisition or qualification jobs"
    );
    bound.workspace
}
fn qualify(workspace: &BoundWorkspace, expected: (u64, u64, u64), exact_flat: bool) {
    let operation = workspace.operation().unwrap();
    let base = operation.workspace().base().unwrap();
    let context = RootContext {
        root: base.identity(),
        scope: base.root().scope(),
        profile: base.root().profile(),
        root_serial: Some(base.root().root_inode().serial()),
    };
    let owner = operation.overlay();
    let done = job(
        owner,
        workspace.route(),
        Command::AcquireOperation { request: 1 },
    );
    let lease = match done.result() {
        Ok(Response::Operation(Some(lease))) => *lease,
        other => panic!("{other:?}"),
    };
    drop(done);
    let scope = IndexedOperationRecordScope {
        owner: lease,
        file_scope: u64::MAX,
    };
    let mut records = IndexedEditRecords::new(owner, scope);
    let mut work = QualificationWork::default();
    // Scope mismatch is checked before operation-record effects.
    let wrong = RootContext {
        profile: ObjectId::for_bytes(b"other profile"),
        ..context
    };
    assert!(matches!(
        qualify_root(operation.client(), &mut records, &wrong, &mut work),
        Err(ContentError::ScopeMismatch { .. })
    ));
    assert_eq!(records.work().calls, 0);
    assert!(records.failure().is_none());
    let before = owner.diagnostics().unwrap();
    let proof = qualify_root(operation.client(), &mut records, &context, &mut work).unwrap();
    assert_eq!(
        (proof.inodes(), proof.directories(), proof.bindings()),
        expected
    );
    assert_eq!(proof.admit(&context).unwrap(), base.root());
    assert!(proof.admit(&wrong).is_err());
    let backing = records.work();
    assert_eq!(backing.calls, work.record_reads + work.record_batches);
    assert_eq!(backing.converted_changes, work.record_changes);
    assert_eq!(backing.terminal_calls, 0);
    assert!(!backing.saturated);
    assert!(work.peak_batch_changes <= 128);
    if exact_flat {
        assert_eq!(work.record_reads, expected.2);
        assert_eq!(work.record_changes, 1 + expected.0 + expected.2);
    }
    let after = owner.diagnostics().unwrap();
    assert_eq!(after.admitted - before.admitted, backing.calls);
    assert_eq!(after.outstanding, before.outstanding);
    assert!(records.failure().is_none());
    assert!(operation.ports().failure().unwrap().is_none());
    println!("INSTALLED_QUALIFY inodes={} directories={} bindings={} inode_pages={} directory_pages={} record_reads={} record_batches={} record_changes={} peak_batch={} owner_jobs={} conversion_peak={} reply_peak={} mount_qualification_jobs=0",
        proof.inodes(),proof.directories(),proof.bindings(),work.inode.pages_read,
        work.directory.pages_read,work.record_reads,work.record_batches,work.record_changes,
        work.peak_batch_changes,backing.calls,backing.peak_conversion_heap_bytes,backing.peak_reply_capacity_bytes);
    let custody = records.into_custody();
    assert_eq!(custody.scope, scope);
    assert!(
        job(owner, workspace.route(), Command::ReleaseOperation(lease))
            .result()
            .is_ok()
    );
    drop(custody);
}
fn close(owner: &Owner, workspace: &BoundWorkspace) {
    assert!(matches!(
        job(&owner.client(), workspace.route(), Command::Close).result(),
        Ok(Response::Done)
    ));
}
#[test]
fn native_installed_mixed_root_is_qualified_as_an_explicit_paid_step() {
    let f = native::Fixture::new("installed-qualification");
    let store = &f.installed.opened.store;
    let workspace = bind(store, &f.owner, f.native.project.branch.branch.id, 71);
    let before = store.work();
    qualify(&workspace, (5, 2, 5), false);
    let read = store.work().object_batches - before.object_batches;
    println!("INSTALLED_QUALIFY_NATIVE install=authenticated record_scope=local_overlay additional_Store_batches={read}");
    close(&f.owner, &workspace);
    drop(workspace);
    f.cleanup();
}
#[test]
fn namespace_size_scales_explicit_qualification_but_not_mount() {
    for count in [1, 1024] {
        let f = copied::Fixture::new(count, &format!("qualification-{count}"));
        assert_eq!(f.count, count);
        let store = open_store(
            f.config.clone(),
            copied::BINDING,
            copied::CURSOR,
            2,
            2 * 1024 * 1024,
            layerfs_storage::ReservationBlocks::default(),
        )
        .unwrap();
        let owner = Owner::start(
            &f.directory.join("overlay.sqlite"),
            ProfileConfig::default(),
            OwnerConfig::default(),
        )
        .unwrap();
        let before = store.work();
        let first = bind(&store, &owner, f.branch, 72);
        let cold = store.work().object_batches - before.object_batches;
        assert!((4..=16).contains(&cold));
        let before = store.work();
        let second = bind(&store, &owner, f.branch, 73);
        assert_eq!(store.work(), before, "warm bound root has no Store demands");
        qualify(&first, (count as u64 + 1, 1, count as u64), true);
        println!(
            "INSTALLED_QUALIFY_SCALE files={count} cold_mount_batches={cold} warm_mount_batches=0"
        );
        close(&owner, &first);
        close(&owner, &second);
        drop((first, second, store));
        owner.stop().unwrap();
        f.cleanup();
    }
}
