//! #41 T6: the Store service's `branch.remove`, `project.delete` and
//! `project.remove_file` admit the Store's intents and report the holders.
#[path = "support/installed_store.rs"]
#[allow(dead_code)]
mod installed;
#[path = "support/sdk.rs"]
#[allow(dead_code)]
mod support;

use layerfs_history::records::{GcIntent, GcIntentKind, GcScopeKind};
use layerfs_history::{HistoryError, PlaceAuthority, PlaceView, WorkspaceId};
use layerfs_protocol::sdk::{SdkRefusal, SdkValue};
use layerfs_store_service::history::{admission, deletion, lookup, removal};
use layerfs_store_service::store::{BindError, BindPhase, BindRequest};
use std::fs;
use support::Fixture;

fn text(value: &str) -> SdkValue {
    SdkValue::text(value)
}

/// An ordered place value in the format the records mint.
fn token(milliseconds: u64, nonce: &str) -> String {
    format!("p1:aa11.worker:{milliseconds:016}:{nonce}")
}

/// Finding 5: a mount intent records who owns it (pid/start, boot, machine,
/// Sandbox and Workspace), so a gone owner can be marked uncertain and a
/// removal reports `Unresolved` instead of `MountsActive` forever. Releasing
/// the row is the owning call's settlement.
#[test]
fn a_mount_intent_records_owner_evidence_and_settles_unresolved() {
    let fixture = Fixture::new("admission-evidence", |source| {
        fs::write(source.join("a.txt"), b"a\n").unwrap();
        1
    });
    fixture.fork("work");
    let stack = lookup::project(&fixture.service.store, "installed").expect("project");
    let branch = lookup::branch(&fixture.service.store, &stack, "work").expect("branch");
    let history = fixture.service.store.history();
    let snapshot = history
        .branch_snapshot(branch.id)
        .expect("snapshot")
        .expect("the Branch");
    let workspace = WorkspaceId::from_authority([7; 32]).expect("workspace");
    let place = layerfs_protocol::daemon::MountPlace::new(Some("sandbox-7"), Some("machine-7"));
    let intent = admission::mount_intent(
        workspace,
        &snapshot,
        Some("installed"),
        &place,
        layerfs_history::PlaceAuthority::default(),
    );
    let runtime = intent.owner_runtime.clone().expect("runtime evidence");
    let boot = intent.owner_boot.clone().expect("boot evidence");
    assert!(runtime.starts_with("pid:"), "{runtime}");
    assert!(boot.starts_with("boot:"), "{boot}");
    assert!(
        boot.contains(&lookup::hex(&[7; 32])),
        "the token names its Workspace: {boot}"
    );
    assert_eq!(intent.machine.as_deref(), Some("machine-7"));
    assert_eq!(intent.sandbox.as_deref(), Some("sandbox-7"));

    history.gc_register(&intent).expect("admit the mount");
    let stored = history
        .gc_intents(GcScopeKind::Branch, branch.id.as_slice())
        .expect("intents");
    assert_eq!(stored.len(), 1);
    assert_eq!(stored[0].owner_runtime.as_deref(), Some(runtime.as_str()));
    assert_eq!(stored[0].owner_boot.as_deref(), Some(boot.as_str()));

    // The production caller's effect when a release is refused: the row is
    // uncertain, so admission reports `Unresolved`.
    history
        .gc_mark_uncertain(&intent.op_id)
        .expect("mark uncertain");
    let refused = removal::remove(&fixture.service.store, &stack, "work")
        .expect_err("an uncertain owner holds");
    assert_eq!(refused.code, "Unresolved", "{refused:?}");
    assert_eq!(
        refused.detail.get_text("op"),
        Some(lookup::hex(&intent.op_id).as_str())
    );

    // The owning call settles the row by releasing it.
    history.gc_release(&intent.op_id).expect("settle");
    removal::remove(&fixture.service.store, &stack, "work").expect("removal after settlement");
    fixture.cleanup();
}

/// The mount builder records the place authority its caller knows, and the
/// owner settles exactly its own row.
#[test]
fn a_mount_intent_records_the_callers_place_authority() {
    let fixture = Fixture::new("admission-authority", |source| {
        fs::write(source.join("a.txt"), b"a\n").unwrap();
        1
    });
    fixture.fork("work");
    let stack = lookup::project(&fixture.service.store, "installed").expect("project");
    let branch = lookup::branch(&fixture.service.store, &stack, "work").expect("branch");
    let history = fixture.service.store.history();
    let snapshot = history
        .branch_snapshot(branch.id)
        .expect("snapshot")
        .expect("the Branch");
    let workspace = WorkspaceId::from_authority([8; 32]).expect("workspace");
    let place = layerfs_protocol::daemon::MountPlace::new(Some("sandbox-8"), Some("machine-8"));
    let authority = PlaceAuthority {
        machine_incarnation: Some("worker:2".to_owned()),
        sandbox_generation: Some("generation:2".to_owned()),
    };
    let intent = admission::mount_intent(
        workspace,
        &snapshot,
        Some("installed"),
        &place,
        authority.clone(),
    );
    assert_eq!(intent.machine_incarnation, authority.machine_incarnation);
    assert_eq!(intent.sandbox_generation, authority.sandbox_generation);
    assert_eq!(intent.project.as_deref(), Some("installed"));

    history.gc_register(&intent).expect("admit the mount");
    let stored = history
        .gc_intents(GcScopeKind::Branch, branch.id.as_slice())
        .expect("intents");
    assert_eq!(stored.len(), 1);
    assert_eq!(stored[0].machine_incarnation, authority.machine_incarnation);
    assert_eq!(stored[0].sandbox_generation, authority.sandbox_generation);

    history
        .gc_release(&intent.op_id)
        .expect("the owner settles its row");
    assert!(history
        .gc_intents(GcScopeKind::Branch, branch.id.as_slice())
        .expect("intents")
        .is_empty());
    fixture.cleanup();
}

/// A mount registration settles no other mount row, as before place
/// evidence: the older placement at the same machine stays until a removal's
/// admission, given the machine's newer ordered incarnation, proves it dead
/// even while its owner process lives; the newer place still holds.
#[test]
fn a_newer_incarnation_settles_the_older_mount_at_removal_not_at_mount() {
    let fixture = Fixture::new("admission-place-dead", |source| {
        fs::write(source.join("a.txt"), b"a\n").unwrap();
        1
    });
    fixture.fork("work");
    let stack = lookup::project(&fixture.service.store, "installed").expect("project");
    let work = lookup::branch(&fixture.service.store, &stack, "work").expect("branch");
    let history = fixture.service.store.history();
    let mount = |tag: u8, incarnation: &str| {
        let snapshot = history
            .branch_snapshot(work.id)
            .expect("snapshot")
            .expect("the Branch");
        admission::mount_intent(
            WorkspaceId::from_authority([tag; 32]).expect("workspace"),
            &snapshot,
            Some("installed"),
            &layerfs_protocol::daemon::MountPlace::new(Some("sandbox-9"), Some("machine-9")),
            PlaceAuthority {
                machine_incarnation: Some(incarnation.to_owned()),
                sandbox_generation: None,
            },
        )
    };
    let stale = mount(0x61, &token(10, "a"));
    history.gc_register(&stale).expect("admit the stale mount");
    let current = mount(0x62, &token(20, "b"));
    history
        .gc_register(&current)
        .expect("admit the current mount");

    // The registration scanned no other mount: both rows stay.
    let listed = history
        .gc_intents(GcScopeKind::Branch, work.id.as_slice())
        .expect("intents");
    assert_eq!(listed.len(), 2);

    let view = || {
        let mut view = PlaceView::new();
        view.set_machine("machine-9", token(20, "b"));
        Some(view)
    };
    let refused = admission::with_place_view(view(), || {
        removal::remove(&fixture.service.store, &stack, "work")
    })
    .expect_err("the current incarnation holds");
    assert_eq!(refused.code, "MountsActive", "{refused:?}");
    history
        .gc_release(&current.op_id)
        .expect("release the current mount");
    // Without the machine's current incarnation the older row's live owner
    // still holds; with it, the removal settles the older placement.
    let refused = removal::remove(&fixture.service.store, &stack, "work")
        .expect_err("no place evidence: the live owner holds");
    assert_eq!(refused.code, "MountsActive", "{refused:?}");
    admission::with_place_view(view(), || {
        removal::remove(&fixture.service.store, &stack, "work")
    })
    .expect("the newer incarnation proves the older mount dead");
    assert!(history
        .gc_intents(GcScopeKind::Branch, work.id.as_slice())
        .expect("intents")
        .is_empty());
    fixture.cleanup();
}

#[test]
fn a_mount_refuses_every_removal_through_the_service() {
    let fixture = Fixture::new("admission-service", |source| {
        fs::write(source.join("a.txt"), b"a\n").unwrap();
        1
    });
    fixture.fork("work");
    let stack = lookup::project(&fixture.service.store, "installed").expect("project");
    let branch = lookup::branch(&fixture.service.store, &stack, "work").expect("branch");
    let mount = GcIntent::shared(
        [9; 16],
        GcIntentKind::Mount,
        GcScopeKind::Branch,
        branch.id.as_slice(),
    )
    .in_project(stack.id.as_slice())
    .named(Some("installed"), Some("work"))
    .placed(Some([9; 32]), Some("sandbox-9"), Some("machine-9"));
    fixture
        .service
        .store
        .history()
        .gc_register(&mount)
        .expect("admit the mount");

    let refused =
        removal::remove(&fixture.service.store, &stack, "work").expect_err("the mount holds");
    assert_eq!(refused.code, "MountsActive", "{refused:?}");
    let SdkValue::List(holders) = refused.detail.get("holders").expect("holders") else {
        panic!("{refused:?}");
    };
    assert_eq!(holders[0].get_text("kind"), Some("mount"));
    assert_eq!(holders[0].get_text("sandbox"), Some("sandbox-9"));
    assert_eq!(holders[0].get_text("machine"), Some("machine-9"));
    let workspace = lookup::hex(&[9; 32]);
    assert_eq!(holders[0].get_text("workspace"), Some(workspace.as_str()));

    let refused = deletion::delete_stack(&fixture.service.store, stack.id, 1_000)
        .expect_err("the mount holds");
    assert_eq!(refused.code, "MountsActive", "{refused:?}");

    let refused = fixture
        .try_call("project.remove_file", vec![("path", text("/a.txt"))])
        .expect_err("the mount holds");
    assert_eq!(refused.code, "MountsActive", "{refused:?}");

    fixture
        .service
        .store
        .history()
        .gc_release(&[9; 16])
        .expect("release the mount");
    removal::remove(&fixture.service.store, &stack, "work").expect("remove after release");
}

/// F43R2-2 (§5.1): a mount checks every scope it names for a live operation,
/// so a live Init or merge row at the Branch's project refuses it `Busy` with
/// the operation's entity, phase and op. Two mounts of one Branch still share
/// the scope, so neither refuses the other.
#[test]
fn a_mount_sees_a_live_init_or_merge_but_shares_with_another_mount() {
    let fixture = Fixture::new("admission-init-mount", |source| {
        fs::write(source.join("a.txt"), b"a\n").unwrap();
        1
    });
    fixture.fork("work");
    let stack = lookup::project(&fixture.service.store, "installed").expect("project");
    let branch = lookup::branch(&fixture.service.store, &stack, "work").expect("branch");
    let history = fixture.service.store.history();
    let snapshot = history
        .branch_snapshot(branch.id)
        .expect("snapshot")
        .expect("the Branch");
    let place = layerfs_protocol::daemon::MountPlace::new(Some("sandbox-7"), Some("machine-7"));
    let mount = |tag: u8| {
        admission::mount_intent(
            WorkspaceId::from_authority([tag; 32]).expect("workspace"),
            &snapshot,
            Some("installed"),
            &place,
            layerfs_history::PlaceAuthority::default(),
        )
    };

    // A live Init or merge is an operation at project scope; a mount of the
    // project's Branch must see it and be refused with its phase and op.
    for (index, (kind, phase)) in [(GcIntentKind::Init, "init"), (GcIntentKind::Merge, "merge")]
        .into_iter()
        .enumerate()
    {
        let op = [0x40 + index as u8; 16];
        let operation = GcIntent::shared(op, kind, GcScopeKind::Project, stack.id.as_slice())
            .named(Some("installed"), None)
            .with_phase(Some(phase));
        history
            .gc_register(&operation)
            .expect("admit the Init/merge");

        let refused = history
            .gc_register(&mount(0x70 + index as u8))
            .expect_err("the live operation holds the project");
        let refused = lookup::history(refused);
        assert_eq!(refused.code, "Busy", "{refused:?}");
        let entity = refused.detail.get("entity").expect("entity detail");
        assert_eq!(entity.get_text("kind"), Some("project"), "{entity:?}");
        assert_eq!(entity.get_text("name"), Some("installed"), "{entity:?}");
        assert_eq!(refused.detail.get_text("phase"), Some(phase));
        assert_eq!(
            refused.detail.get_text("op"),
            Some(lookup::hex(&op).as_str())
        );

        history.gc_release(&op).expect("release the Init/merge");
        history.gc_release(&mount(0x70 + index as u8).op_id).ok();
    }

    // Mounts are shared: two live mounts of the Branch do not refuse each
    // other, so the OPS scan must keep `mount` out of its kind filter.
    let first = mount(0x81);
    let second = mount(0x82);
    history.gc_register(&first).expect("the first mount");
    history.gc_register(&second).expect("the second mount");
    history.gc_release(&first.op_id).expect("release the first");
    history
        .gc_release(&second.op_id)
        .expect("release the second");
    fixture.cleanup();
}

/// A placed bind records the Sandbox generation its control caller read from
/// the Sandbox record, beside the Sandbox and machine names, so admission can
/// later compare it.
#[test]
fn a_placed_bind_records_the_sandbox_generation() {
    let fixture = Fixture::new("admission-placed-generation", |source| {
        fs::write(source.join("a.txt"), b"a\n").unwrap();
        1
    });
    fixture.fork("work");
    let stack = lookup::project(&fixture.service.store, "installed").expect("project");
    let branch = lookup::branch(&fixture.service.store, &stack, "work").expect("branch");
    let history = fixture.service.store.history();
    let incarnation = "p1:aa11.worker:10:aa";
    let generation = "p1:aa11.sandbox:10:aa";
    let place = layerfs_protocol::daemon::MountPlace::new(Some("sandbox-21"), Some("machine-21"))
        .with_generation(Some(generation))
        .with_machine_incarnation(Some(incarnation));
    let bound = fixture
        .service
        .store
        .bind_placed(
            fixture.owner.client(),
            BindRequest {
                workspace: WorkspaceId::from_authority([21; 32]).expect("workspace"),
                branch: branch.id,
            },
            place,
        )
        .expect("a placed bind");
    let intents = history
        .gc_intents(GcScopeKind::Branch, branch.id.as_slice())
        .expect("intents");
    assert_eq!(intents.len(), 1);
    assert_eq!(intents[0].sandbox.as_deref(), Some("sandbox-21"));
    assert_eq!(intents[0].machine.as_deref(), Some("machine-21"));
    assert_eq!(intents[0].machine_incarnation.as_deref(), Some(incarnation));
    assert_eq!(intents[0].sandbox_generation.as_deref(), Some(generation));
    // A bind by Branch id names the project, so the audit finds the row.
    assert_eq!(intents[0].project.as_deref(), Some("installed"));
    assert_eq!(intents[0].branch.as_deref(), Some("work"));
    bound.workspace.close().expect("close the Workspace");
    assert!(history
        .gc_intents(GcScopeKind::Branch, branch.id.as_slice())
        .expect("intents")
        .is_empty());
    fixture.cleanup();
}

/// A mount whose Sandbox is later retired (its machine tombstone names the
/// generation the mount recorded) is settled at admission: the Branch removal
/// proceeds without an operator and leaves no live row.
#[test]
fn a_tombstoned_sandbox_generation_settles_its_mount() {
    let fixture = Fixture::new("admission-retired-generation", |source| {
        fs::write(source.join("a.txt"), b"a\n").unwrap();
        1
    });
    fixture.fork("work");
    let stack = lookup::project(&fixture.service.store, "installed").expect("project");
    let branch = lookup::branch(&fixture.service.store, &stack, "work").expect("branch");
    let history = fixture.service.store.history();
    let incarnation = "p1:aa11.worker:10:aa";
    let generation = "p1:aa11.sandbox:10:aa";
    let place = layerfs_protocol::daemon::MountPlace::new(Some("sandbox-22"), Some("machine-22"))
        .with_generation(Some(generation))
        .with_machine_incarnation(Some(incarnation));
    let bound = fixture
        .service
        .store
        .bind_placed(
            fixture.owner.client(),
            BindRequest {
                workspace: WorkspaceId::from_authority([22; 32]).expect("workspace"),
                branch: branch.id,
            },
            place,
        )
        .expect("a placed bind");
    assert_eq!(
        history
            .gc_intents(GcScopeKind::Branch, branch.id.as_slice())
            .expect("intents")[0]
            .sandbox_generation
            .as_deref(),
        Some(generation)
    );

    // The machine's tombstone names the generation the mount recorded and no
    // current record remains: admission proves the mount dead, so the removal
    // settles it instead of reporting `MountsActive`.
    let mut view = PlaceView::new();
    view.set_machine("machine-22", incarnation);
    view.set_sandbox_retired("machine-22", "sandbox-22", generation);
    let removed = admission::with_place_view(Some(view), || {
        removal::remove(&fixture.service.store, &stack, "work")
    })
    .expect("the retired Sandbox settles its mount");
    assert_eq!(removed.get_text("kind"), Some("Removed"));
    assert!(history
        .gc_intents(GcScopeKind::Branch, branch.id.as_slice())
        .expect("intents")
        .is_empty());
    bound.workspace.close().expect("the settled mount closes");
    fixture.cleanup();
}

/// A private Store-call bind knows no place: its intent records no name,
/// generation, lease or fence and admission falls back to the owner tokens,
/// which still refuse a removal while that owner lives.
#[test]
fn a_private_bind_records_no_place_authority() {
    let fixture = Fixture::new("admission-private-place", |source| {
        fs::write(source.join("a.txt"), b"a\n").unwrap();
        1
    });
    fixture.fork("work");
    let stack = lookup::project(&fixture.service.store, "installed").expect("project");
    let branch = lookup::branch(&fixture.service.store, &stack, "work").expect("branch");
    let history = fixture.service.store.history();
    let bound = fixture
        .service
        .store
        .bind(
            fixture.owner.client(),
            BindRequest {
                workspace: WorkspaceId::from_authority([23; 32]).expect("workspace"),
                branch: branch.id,
            },
        )
        .expect("a private bind");
    let intents = history
        .gc_intents(GcScopeKind::Branch, branch.id.as_slice())
        .expect("intents");
    assert_eq!(intents.len(), 1);
    assert_eq!(intents[0].machine, None);
    assert_eq!(intents[0].sandbox, None);
    assert_eq!(intents[0].machine_incarnation, None);
    assert_eq!(intents[0].sandbox_generation, None);
    assert!(intents[0].owner_runtime.is_some());
    assert!(intents[0].owner_boot.is_some());
    let refused = removal::remove(&fixture.service.store, &stack, "work")
        .expect_err("the private mount holds by its owner tokens");
    assert_eq!(refused.code, "MountsActive", "{refused:?}");
    bound
        .workspace
        .close()
        .expect("close the private Workspace");
    removal::remove(&fixture.service.store, &stack, "work").expect("remove after release");
    fixture.cleanup();
}

/// A catalog that answers from `inner` but loses one admission write's
/// answer, as persistence does at a lost reply.
mod fault {
    use layerfs_history::records::GcIntent;
    use layerfs_history::*;

    pub struct Faulty<'a> {
        pub inner: &'a dyn HistoryCatalog,
        pub fail_register: bool,
        pub fail_release: bool,
        /// A definite refusal the registration answers instead.
        pub refuse_register: Option<HistoryError>,
    }

    macro_rules! forward {
        ($($name:ident($($arg:ident: $ty:ty),*) -> $out:ty;)*) => {$(
            fn $name(&self, $($arg: $ty),*) -> $out { self.inner.$name($($arg),*) }
        )*};
    }

    impl HistoryCatalog for Faulty<'_> {
        forward! {
            catalog_id() -> CatalogId;
            incarnation() -> u64;
            layer_stack(id: LayerStackId) -> HistoryResult<Option<LayerStackRecord>>;
            layer_stacks(page: &Page) -> HistoryResult<PageResult<LayerStackRecord>>;
            branch(id: BranchId) -> HistoryResult<Option<BranchRecord>>;
            branch_named(stack: LayerStackId, name: &HistoryName) -> HistoryResult<Option<BranchRecord>>;
            branch_snapshot(id: BranchId) -> HistoryResult<Option<BranchSnapshot>>;
            branches(stack: LayerStackId, page: &Page) -> HistoryResult<PageResult<BranchRecord>>;
            commit(id: CommitId) -> HistoryResult<Option<CommitRecord>>;
            layer(id: LayerId) -> HistoryResult<Option<LayerRecord>>;
            commit_history(request: &CommitHistoryRequest) -> HistoryResult<PageResult<CommitRecord>>;
            layer_history(request: &LayerHistoryRequest) -> HistoryResult<PageResult<LayerRecord>>;
            initialize_layerstack(request: &StackInitialization) -> HistoryResult<LayerStackRecord>;
            fork(request: &ForkRequest) -> HistoryResult<BranchSnapshot>;
            merge_base(left: NodeId, right: NodeId) -> HistoryResult<Option<NodeId>>;
            is_ancestor(ancestor: NodeId, descendant: NodeId) -> HistoryResult<bool>;
            commit_branch(request: &CommitRequest) -> HistoryResult<CommitOutcome>;
            add_layer(request: &AddLayerRequest) -> HistoryResult<AddLayerOutcome>;
            merge_branch(request: &BranchMergeRequest) -> HistoryResult<BranchSnapshot>;
            reserve_inodes(request: &ReserveRequest) -> HistoryResult<Reservation>;
        }

        fn gc_register(&self, intent: &GcIntent) -> HistoryResult<()> {
            if self.fail_register {
                return Err(HistoryError::UnknownOutcome);
            }
            if let Some(refusal) = &self.refuse_register {
                return Err(refusal.clone());
            }
            self.inner.gc_register(intent)
        }

        fn gc_release(&self, op_id: &[u8; 16]) -> HistoryResult<()> {
            if self.fail_release {
                return Err(HistoryError::UnknownOutcome);
            }
            self.inner.gc_release(op_id)
        }
    }

    /// A writer catalog whose registration is refused `Unsupported(reason)`:
    /// the PostgreSQL provider's own reason on a transaction-pooled
    /// connection (§5.2), or the trait default's `NO_ADMISSION` of a catalog
    /// without admission.
    pub struct Pooled {
        pub inner: std::sync::Arc<dyn HistoryCatalog>,
        pub reason: &'static str,
    }

    impl HistoryCatalog for Pooled {
        forward! {
            catalog_id() -> CatalogId;
            incarnation() -> u64;
            layer_stack(id: LayerStackId) -> HistoryResult<Option<LayerStackRecord>>;
            layer_stacks(page: &Page) -> HistoryResult<PageResult<LayerStackRecord>>;
            branch(id: BranchId) -> HistoryResult<Option<BranchRecord>>;
            branch_named(stack: LayerStackId, name: &HistoryName) -> HistoryResult<Option<BranchRecord>>;
            branch_snapshot(id: BranchId) -> HistoryResult<Option<BranchSnapshot>>;
            branches(stack: LayerStackId, page: &Page) -> HistoryResult<PageResult<BranchRecord>>;
            commit(id: CommitId) -> HistoryResult<Option<CommitRecord>>;
            layer(id: LayerId) -> HistoryResult<Option<LayerRecord>>;
            commit_history(request: &CommitHistoryRequest) -> HistoryResult<PageResult<CommitRecord>>;
            layer_history(request: &LayerHistoryRequest) -> HistoryResult<PageResult<LayerRecord>>;
            initialize_layerstack(request: &StackInitialization) -> HistoryResult<LayerStackRecord>;
            fork(request: &ForkRequest) -> HistoryResult<BranchSnapshot>;
            merge_base(left: NodeId, right: NodeId) -> HistoryResult<Option<NodeId>>;
            is_ancestor(ancestor: NodeId, descendant: NodeId) -> HistoryResult<bool>;
            commit_branch(request: &CommitRequest) -> HistoryResult<CommitOutcome>;
            add_layer(request: &AddLayerRequest) -> HistoryResult<AddLayerOutcome>;
            merge_branch(request: &BranchMergeRequest) -> HistoryResult<BranchSnapshot>;
            reserve_inodes(request: &ReserveRequest) -> HistoryResult<Reservation>;
        }

        fn gc_register(&self, _intent: &GcIntent) -> HistoryResult<()> {
            Err(HistoryError::Unsupported(self.reason))
        }
    }
}

/// A Store over `host`'s SQLite file whose writer catalog is [`fault::Pooled`].
fn pooled_store(
    host: &installed::Fixture,
    reason: &'static str,
) -> std::sync::Arc<layerfs_store_service::store::Store> {
    use layerfs_persistence::SqliteHandles;
    use layerfs_store_service::store::{ReaderFactory, Store, StoreReader};
    use std::sync::Arc;
    let writer =
        SqliteHandles::open_writable(host.config.clone(), installed::BINDING, installed::CURSOR)
            .unwrap();
    let config = host.config.clone();
    let factory: ReaderFactory = Arc::new(move |_| {
        let read =
            SqliteHandles::open_read_only(config.clone(), installed::BINDING, installed::CURSOR)?;
        Ok(StoreReader::new(
            layerfs_storage::Storage::new(read.storage)?,
            Arc::new(read.history),
        ))
    });
    let control = factory(0).unwrap();
    let history = Arc::new(fault::Pooled {
        inner: Arc::new(writer.history),
        reason,
    });
    Arc::new(
        Store::new(
            writer.storage,
            history,
            control,
            factory,
            0,
            layerfs_storage::ReservationBlocks::default(),
            2,
        )
        .unwrap(),
    )
}

/// #768, #776 C1: a provider whose registration is refused with its own
/// `Unsupported` (PostgreSQL on a transaction pooler, §5.2) refuses the
/// mount; only a catalog without admission (`NO_ADMISSION`) mounts with no
/// intent. The bind path's twin of `a_pooled_connection_refuses_a_shared_intent`.
#[test]
fn a_pooled_registration_refuses_the_mount() {
    let host = installed::Fixture::built("admission-pooled-mount", |source| {
        fs::write(source.join("a.txt"), b"a\n").unwrap();
        1
    });
    let owner = layerfs_store_service::Owner::start(
        &host.directory.join("overlay.sqlite"),
        layerfs_overlay::ProfileConfig::default(),
        layerfs_store_service::OwnerConfig::default(),
    )
    .unwrap();
    let request = |n: u8| BindRequest {
        branch: host.branch,
        workspace: WorkspaceId::from_authority([n; 32]).unwrap(),
    };

    let store = pooled_store(&host, "transaction-pooled PostgreSQL connection");
    let refused = store
        .bind(owner.client(), request(31))
        .err()
        .expect("a pooled registration refuses the mount");
    assert!(
        matches!(
            refused.error,
            BindError::History(layerfs_history::HistoryError::Unsupported(
                "transaction-pooled PostgreSQL connection"
            ))
        ),
        "{:?}",
        refused.error
    );
    assert_eq!(refused.phase, BindPhase::Snapshot);
    drop(store);

    let store = pooled_store(&host, layerfs_history::error::NO_ADMISSION);
    let bound = store
        .bind(owner.client(), request(32))
        .expect("a provider without admission mounts")
        .workspace;
    let namespace = bound.route().namespace();
    bound.close().expect("close the Workspace");
    owner.client().retire_namespace(namespace).unwrap();
    drop(store);
    owner.stop().unwrap();
    host.cleanup();
}

fn unknown_names(refused: &SdkRefusal, phase: &str) {
    assert_eq!(refused.code, "Unknown", "{refused:?}");
    let entity = refused.detail.get("entity").expect("entity");
    assert_eq!(entity.get_text("kind"), Some("project"), "{entity:?}");
    assert_eq!(entity.get_text("name"), Some("installed"), "{entity:?}");
    assert_eq!(refused.detail.get_text("phase"), Some(phase));
    let op = refused.detail.get_text("op").expect("the operation");
    assert_eq!(op.len(), 32, "{op}");
    assert!(op.bytes().all(|byte| byte.is_ascii_hexdigit()), "{op}");
}

/// F43R3-2: a shared Init/merge intent whose registration or release loses
/// its answer is `Unknown` naming the project, phase and operation it held
/// (spec §10 A6), never the bare `{}` persistence detail.
#[test]
fn a_shared_intent_unknown_outcome_names_its_project_phase_and_op() {
    let fixture = Fixture::new("admission-shared-unknown", |source| {
        fs::write(source.join("a.txt"), b"a\n").unwrap();
        1
    });
    let stack = lookup::project(&fixture.service.store, "installed").expect("project");
    let catalog = fixture.service.store.history();

    let lost = fault::Faulty {
        inner: catalog,
        fail_register: true,
        fail_release: false,
        refuse_register: None,
    };
    let refused = admission::SharedIntent::begin(&lost, GcIntentKind::Merge, &stack, Some("work"))
        .err()
        .expect("a lost registration is refused");
    unknown_names(&refused, "merge");

    let lost = fault::Faulty {
        inner: catalog,
        fail_register: false,
        fail_release: true,
        refuse_register: None,
    };
    let intent = admission::SharedIntent::begin(&lost, GcIntentKind::Init, &stack, None)
        .expect("a registration that answers");
    let refused = intent.release().expect_err("a lost release is refused");
    unknown_names(&refused, "init");
    // The refused release left the row live; settle it through the real
    // catalog so the fixture ends clean.
    for intent in catalog
        .gc_intents(GcScopeKind::Project, stack.id.as_slice())
        .expect("intents")
    {
        catalog.gc_release(&intent.op_id).expect("settle the row");
    }
    fixture.cleanup();
}

/// #776 C1: only a catalog with no GC admission at all lets a merge proceed
/// unadmitted. A real provider's own `Unsupported`, the transaction-pooled
/// PostgreSQL connection that cannot keep a session key, refuses the merge
/// instead of running it with no intent row.
#[test]
fn a_pooled_connection_refuses_a_shared_intent() {
    let fixture = Fixture::new("admission-pooled", |source| {
        fs::write(source.join("a.txt"), b"a\n").unwrap();
        1
    });
    let stack = lookup::project(&fixture.service.store, "installed").expect("project");
    let catalog = fixture.service.store.history();

    let pooled = fault::Faulty {
        inner: catalog,
        fail_register: false,
        fail_release: false,
        refuse_register: Some(HistoryError::Unsupported(
            "transaction-pooled PostgreSQL connection",
        )),
    };
    let refused =
        admission::SharedIntent::begin(&pooled, GcIntentKind::Merge, &stack, Some("work"))
            .err()
            .expect("a pooled connection refuses the merge");
    assert_eq!(refused.code, "Unsupported", "{refused:?}");

    let unadmitted = fault::Faulty {
        inner: catalog,
        fail_register: false,
        fail_release: false,
        refuse_register: Some(HistoryError::Unsupported(
            layerfs_history::error::NO_ADMISSION,
        )),
    };
    let intent =
        admission::SharedIntent::begin(&unadmitted, GcIntentKind::Merge, &stack, Some("work"))
            .expect("a catalog without admission proceeds");
    intent.release().expect("nothing to release");
    assert!(catalog
        .gc_intents(GcScopeKind::Project, stack.id.as_slice())
        .expect("intents")
        .is_empty());
    fixture.cleanup();
}

/// A reconcile settles exactly the mount its newer ordered incarnation proves
/// dead, answering the wire shape the worker reports; a second reconcile at
/// the same value settles nothing.
#[test]
fn the_service_reconcile_settles_the_dead_mount() {
    let fixture = Fixture::new("admission-place-writers", |source| {
        fs::write(source.join("a.txt"), b"a\n").unwrap();
        1
    });
    fixture.fork("work");
    let stack = lookup::project(&fixture.service.store, "installed").expect("project");
    let branch = lookup::branch(&fixture.service.store, &stack, "work").expect("branch");
    let history = fixture.service.store.history();
    let snapshot = history
        .branch_snapshot(branch.id)
        .expect("snapshot")
        .expect("the Branch");
    let workspace = WorkspaceId::from_authority([9; 32]).expect("workspace");
    let place = layerfs_protocol::daemon::MountPlace::new(Some("sandbox-9"), Some("machine-9"));
    let authority = PlaceAuthority {
        machine_incarnation: Some(token(10, "a")),
        sandbox_generation: Some(token(10, "g")),
    };
    let intent =
        admission::mount_intent(workspace, &snapshot, Some("installed"), &place, authority);
    history.gc_register(&intent).expect("admit the mount");

    let newer = token(20, "b");
    let reconcile =
        admission::reconcile_machine(history, "machine-9", &newer).expect("the reconcile");
    assert_eq!(reconcile.settled, vec![intent.op_id]);
    assert_eq!((reconcile.held, reconcile.uncertain), (0, 0));
    let value = admission::reconcile_value(&reconcile);
    assert_eq!(value.get_text("machine"), Some("machine-9"));
    assert_eq!(value.get_text("incarnation"), Some(newer.as_str()));
    let settled = value.get("settled").expect("settled list");
    assert_eq!(
        settled,
        &SdkValue::List(vec![SdkValue::text(lookup::hex(&intent.op_id))])
    );
    assert_eq!(value.get_int("held"), Some(0));

    let again =
        admission::reconcile_machine(history, "machine-9", &newer).expect("the second reconcile");
    assert!(again.settled.is_empty());
    fixture.cleanup();
}
