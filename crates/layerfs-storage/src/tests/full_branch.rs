//! Target Full Branch CAS, receipt, staging, ancestry, and role qualification.

use super::fixture::test_path;
use crate::full::record_id::transition_identity;
use crate::{
    branch_push_page_digest, derive_id, BranchAncestry, BranchHead, BranchId, BranchPushBundle,
    BranchPushIdentityBuilder, BranchPushOutcome, BranchPushRequest, EngineError, FullStorage,
    LayerId, LayerStackId, OperationId, OperationVersionId, PushedBranchRollback, PushedLayerStack,
    PushedOperation, RequestId, SyncTransferCounters, VersionRef,
};
use layerfs_core::{encode_bytes_object, ObjectId, ObjectKind};
use rusqlite::params;

struct Fixture {
    storage: FullStorage,
    pages: Vec<BranchPushBundle>,
    request: BranchPushRequest,
    branch: BranchId,
    head: BranchHead,
    ancestry: BranchAncestry,
}

fn object(storage: &FullStorage, value: &[u8]) -> ObjectId {
    let canonical = encode_bytes_object(value).unwrap();
    let id = ObjectId::for_bytes(&canonical);
    storage
        .lock_connection()
        .unwrap()
        .execute(
            "INSERT INTO layerfs_objects
             (object_id, kind, canonical_length, canonical_bytes) VALUES (?1, ?2, ?3, ?4)",
            params![
                id.as_bytes(),
                ObjectKind::Bytes as u8,
                canonical.len() as i64,
                canonical
            ],
        )
        .unwrap();
    id
}

fn operation(
    operation: OperationId,
    request: RequestId,
    sequence: u64,
    base: VersionRef,
    parent: Option<OperationVersionId>,
    root: ObjectId,
) -> PushedOperation {
    let version = OperationVersionId::from_bytes(derive_id(
        b"operation-version",
        &[operation.as_bytes(), request.as_bytes(), root.as_bytes()],
    ));
    let payload = vec![u8::try_from(sequence + 1).unwrap()];
    let delta = transition_identity(base.root(), root, &payload);
    PushedOperation {
        operation_id: operation,
        operation_sequence: sequence,
        expected_branch_generation: sequence,
        base,
        operation_version_id: version,
        version_sequence: sequence,
        parent_operation_version_id: parent,
        root,
        release: None,
        operation_delta_id: derive_id(
            b"operation-delta",
            &[operation.as_bytes(), version.as_bytes(), &delta],
        ),
        transition_delta_id: delta,
        transition_payload: payload,
        request_id: request,
        before_generation: sequence,
        after_generation: sequence + 1,
    }
}

fn fixture(path: &std::path::Path) -> Fixture {
    let storage = FullStorage::create_durable(path).unwrap();
    let root0 = object(&storage, b"branch-root-0");
    let root1 = object(&storage, b"branch-root-1");
    let root2 = object(&storage, b"branch-root-2");
    let stack = LayerStackId::from_bytes([0x11; 32]);
    let layer = LayerId::from_bytes([0x12; 32]);
    let stack_head = storage
        .bootstrap_layer_stack(stack, layer, "base", root0)
        .unwrap();
    let branch = BranchId::from_bytes([0x13; 32]);
    let ancestry = BranchAncestry {
        immediate_parent_branch_id: None,
        fork_operation_id: None,
        fork_operation_version_id: None,
        fork_root: root0,
        origin_layer_stack_id: stack,
        origin_layer_id: layer,
        depth: 0,
    };
    let first = operation(
        OperationId::from_bytes([0x21; 32]),
        RequestId::from_bytes([0x22; 32]),
        0,
        VersionRef::Layer {
            layer_stack_id: stack,
            layer_id: layer,
            root: root0,
        },
        None,
        root1,
    );
    let head1 = BranchHead {
        branch_id: branch,
        generation: 1,
        operation_version_id: Some(first.operation_version_id),
        root: root1,
    };
    let second = operation(
        OperationId::from_bytes([0x23; 32]),
        RequestId::from_bytes([0x24; 32]),
        1,
        VersionRef::OperationVersion {
            branch_id: branch,
            operation_version_id: first.operation_version_id,
            root: root1,
        },
        Some(first.operation_version_id),
        root2,
    );
    let head = BranchHead {
        branch_id: branch,
        generation: 2,
        operation_version_id: Some(second.operation_version_id),
        root: root2,
    };
    let origin_stack = PushedLayerStack {
        name: "base".into(),
        base: Some(stack_head),
        head: stack_head,
        complete: true,
        layers: Vec::new(),
        transitions: Vec::new(),
    };
    let pages = vec![
        BranchPushBundle {
            name: Some("work".into()),
            ancestry,
            base: None,
            head: head1,
            complete: false,
            operations: vec![first],
            child_merges: Vec::new(),
            rollbacks: Vec::new(),
            origin_stack: origin_stack.clone(),
            dependencies: Vec::new(),
        },
        BranchPushBundle {
            name: Some("work".into()),
            ancestry,
            base: Some(head1),
            head,
            complete: true,
            operations: vec![second],
            child_merges: Vec::new(),
            rollbacks: Vec::new(),
            origin_stack,
            dependencies: Vec::new(),
        },
    ];
    let request_id = RequestId::from_bytes([0x31; 32]);
    let transfer_id = RequestId::from_bytes([0x32; 32]);
    let counters = SyncTransferCounters::default();
    let digest = candidate_digest(transfer_id, &pages, counters);
    let request = BranchPushRequest {
        request_id,
        transfer_id,
        candidate_digest: digest,
        expected: None,
        counters,
    };
    stage(&storage, request, &pages);
    Fixture {
        storage,
        pages,
        request,
        branch,
        head,
        ancestry,
    }
}

fn candidate_digest(
    transfer: RequestId,
    pages: &[BranchPushBundle],
    counters: SyncTransferCounters,
) -> [u8; 32] {
    let mut identity = BranchPushIdentityBuilder::new(transfer);
    for (sequence, page) in pages.iter().enumerate() {
        let sequence = u64::try_from(sequence).unwrap();
        let data = RequestId::from_bytes([0x40 + u8::try_from(sequence).unwrap(); 32]);
        let encoded = serde_json::to_vec(page).unwrap();
        identity
            .absorb_page(
                sequence,
                branch_push_page_digest(
                    transfer,
                    sequence,
                    data,
                    page.head.branch_id,
                    &encoded,
                    counters,
                ),
            )
            .unwrap();
    }
    identity.finish(pages.last().unwrap().head)
}

fn stage(storage: &FullStorage, request: BranchPushRequest, pages: &[BranchPushBundle]) {
    for (sequence, page) in pages.iter().enumerate() {
        let sequence = u64::try_from(sequence).unwrap();
        let data = RequestId::from_bytes([0x40 + u8::try_from(sequence).unwrap(); 32]);
        storage
            .stage_verified_branch_push_page(
                request.transfer_id,
                sequence,
                data,
                page,
                SyncTransferCounters::default(),
            )
            .unwrap();
    }
}

#[test]
fn full_branch_push_is_one_cas_with_exact_replay_and_conflict() {
    let path = test_path();
    let fixture = fixture(&path);
    fixture.storage.reset_counters().unwrap();
    assert_eq!(
        fixture
            .storage
            .commit_verified_ordinary_branch_push(fixture.request, fixture.branch),
        Ok(BranchPushOutcome::DurablyAccepted {
            head: fixture.head,
            reconciled: false
        })
    );
    assert_eq!(
        fixture.storage.authoritative_branch_head(fixture.branch),
        Ok(Some(fixture.head))
    );
    assert_eq!(
        fixture
            .storage
            .counters()
            .unwrap()
            .durable_head_transactions,
        1
    );
    assert_eq!(
        fixture
            .storage
            .authoritative_branch_ancestry(fixture.branch),
        Ok(Some(fixture.ancestry))
    );
    assert_eq!(
        fixture
            .storage
            .commit_verified_ordinary_branch_push(fixture.request, fixture.branch),
        Ok(BranchPushOutcome::DurablyAccepted {
            head: fixture.head,
            reconciled: true
        })
    );
    assert_eq!(
        fixture
            .storage
            .reconcile_verified_ordinary_branch_push(fixture.request, fixture.branch),
        Ok(BranchPushOutcome::DurablyAccepted {
            head: fixture.head,
            reconciled: true
        })
    );
    let mut altered = fixture.request;
    altered.candidate_digest[0] ^= 1;
    assert!(matches!(
        fixture
            .storage
            .commit_verified_ordinary_branch_push(altered, fixture.branch),
        Err(EngineError::InvalidRecord("Full staged Push candidate"))
    ));
    assert_eq!(
        fixture.storage.authoritative_branch_head(fixture.branch),
        Ok(Some(fixture.head))
    );
    assert_eq!(
        fixture
            .storage
            .sync_custody_rows(fixture.request.transfer_id, "push"),
        Ok(5)
    );
    assert_eq!(
        fixture
            .storage
            .abort_sync_transfer(fixture.request.transfer_id, "push"),
        Ok(5)
    );
    assert_eq!(
        fixture
            .storage
            .sync_custody_rows(fixture.request.transfer_id, "push"),
        Ok(0)
    );
    assert!(matches!(
        fixture.storage.reconcile_verified_ordinary_branch_push(
            fixture.request,
            fixture.branch
        ),
        Ok(BranchPushOutcome::DurablyAccepted {
            head,
            reconciled: true
        }) if head == fixture.head
    ));
    let mut conflict = fixture.request;
    conflict.request_id = RequestId::from_bytes([0x33; 32]);
    conflict.transfer_id = RequestId::from_bytes([0x34; 32]);
    conflict.candidate_digest =
        candidate_digest(conflict.transfer_id, &fixture.pages, conflict.counters);
    stage(&fixture.storage, conflict, &fixture.pages);
    for _ in 0..2 {
        assert_eq!(
            fixture
                .storage
                .commit_verified_ordinary_branch_push(conflict, fixture.branch),
            Ok(BranchPushOutcome::Conflict {
                actual: Some(fixture.head)
            })
        );
    }
    assert_eq!(
        fixture.storage.authoritative_branch_head(fixture.branch),
        Ok(Some(fixture.head))
    );
    let connection = fixture.storage.lock_connection().unwrap();
    for table in [
        "layerfs_operations",
        "layerfs_operation_versions",
        "layerfs_branch_transitions",
    ] {
        let count: i64 = connection
            .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(count, 2, "{table}");
    }
    drop(connection);
    drop(fixture.storage);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn full_branch_rejects_special_history_and_cache_authority() {
    let path = test_path();
    let mut fixture = fixture(&path);
    let before = fixture.pages[1].base.unwrap();
    fixture.pages[1].operations.clear();
    fixture.pages[1].head = BranchHead {
        generation: before.generation + 1,
        operation_version_id: before.operation_version_id,
        root: before.root,
        ..before
    };
    fixture.pages[1].rollbacks.push(PushedBranchRollback {
        before_operation_version_id: before.operation_version_id.unwrap(),
        target_operation_version_id: before.operation_version_id.unwrap(),
        target_root: before.root,
        request_id: RequestId::from_bytes([0x61; 32]),
        before_generation: before.generation,
        after_generation: before.generation + 1,
    });
    fixture
        .storage
        .lock_connection()
        .unwrap()
        .execute(
            "DELETE FROM layerfs_branch_push_pages WHERE transfer_id = ?1",
            params![fixture.request.transfer_id.as_bytes()],
        )
        .unwrap();
    fixture.request.candidate_digest = candidate_digest(
        fixture.request.transfer_id,
        &fixture.pages,
        fixture.request.counters,
    );
    stage(&fixture.storage, fixture.request, &fixture.pages);
    assert!(fixture
        .storage
        .commit_verified_ordinary_branch_push(fixture.request, fixture.branch)
        .is_err());
    drop(fixture.storage);
    std::fs::remove_file(path).unwrap();

    let cache_path = test_path();
    let cache = FullStorage::create_cache(&cache_path, [0x71; 32]).unwrap();
    assert!(matches!(
        cache.authoritative_branch_head(BranchId::from_bytes([0x72; 32])),
        Err(EngineError::InvalidRecord("FullStorage authority role"))
    ));
    drop(cache);
    std::fs::remove_file(cache_path).unwrap();
}

#[test]
fn full_branch_cas_rechecks_the_persisted_page_set() {
    let path = test_path();
    let fixture = fixture(&path);
    fixture
        .storage
        .lock_connection()
        .unwrap()
        .execute(
            "UPDATE layerfs_branch_push_pages SET bundle = X'00'
             WHERE transfer_id = ?1 AND page_sequence = 1",
            params![fixture.request.transfer_id.as_bytes()],
        )
        .unwrap();
    assert!(matches!(
        fixture
            .storage
            .commit_verified_ordinary_branch_push(fixture.request, fixture.branch),
        Err(EngineError::InvalidRecord("Full staged Push bundle"))
    ));
    assert_eq!(
        fixture.storage.authoritative_branch_head(fixture.branch),
        Ok(None)
    );
    let receipt: bool = fixture
        .storage
        .lock_connection()
        .unwrap()
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM layerfs_sync_receipts WHERE request_id = ?1)",
            params![fixture.request.request_id.as_bytes()],
            |row| row.get(0),
        )
        .unwrap();
    assert!(!receipt);
    drop(fixture.storage);
    std::fs::remove_file(path).unwrap();
}
