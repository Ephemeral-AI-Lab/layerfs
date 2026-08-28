use super::*;
use layerfs_storage::{
    BranchHead, BranchPushOutcome, BranchPushRequest, OperationVersionId, SyncTransferCounters,
};
use rusqlite::{params, Connection};
use std::path::PathBuf;

#[derive(Debug, Eq, PartialEq)]
struct ReceiptRow {
    durable_storage_id: Vec<u8>,
    direction: String,
    candidate_kind: String,
    candidate_id: Vec<u8>,
    identity_version: Option<i64>,
    transfer_id: Option<Vec<u8>>,
    candidate_digest: Option<Vec<u8>>,
    expected_head_id: Option<Vec<u8>>,
    expected_generation: Option<i64>,
    expected_root_id: Option<Vec<u8>>,
    result: String,
    accepted_head_id: Option<Vec<u8>>,
    accepted_generation: Option<i64>,
    accepted_root_id: Option<Vec<u8>>,
    unique_bytes: i64,
    resumed_bytes: i64,
    retransmitted_bytes: i64,
    reconciliation_result: Option<String>,
}

struct ReplayFixture {
    directory: PathBuf,
    database: PathBuf,
    engine: Engine,
    request: BranchPushRequest,
    expected: BranchHead,
    accepted: BranchHead,
}

impl ReplayFixture {
    fn new(seed: u8) -> Self {
        let directory = std::env::temp_dir().join(format!(
            "layerfs-replay-receipt-{seed}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&directory).unwrap();
        let database = directory.join("store.sqlite");
        let engine = Engine::open_with_mode(&database, IntegrityMode::TrustedLocalDev).unwrap();
        let base_root = valid_empty_root_with_seed(&engine, [seed; 32]);
        let stack = engine
            .product_create_layer_stack(
                LayerStackId::from_bytes([seed.wrapping_add(1); 32]),
                LayerId::from_bytes([seed.wrapping_add(2); 32]),
                "receipt",
                base_root,
            )
            .unwrap();
        let expected = engine
            .product_create_top_level_branch(
                BranchId::from_bytes([seed.wrapping_add(3); 32]),
                Some("receipt"),
                stack,
            )
            .unwrap();
        let candidate_root = valid_empty_root_with_seed(&engine, [seed.wrapping_add(4); 32]);
        let operation_id = OperationId::from_bytes([seed.wrapping_add(5); 32]);
        engine
            .product_begin_operation(
                operation_id,
                expected,
                LeaseId::from_bytes([seed.wrapping_add(6); 32]),
            )
            .unwrap();
        let accepted = match engine
            .product_operation_commit(OperationCandidate {
                operation_id,
                expected,
                candidate_root,
                normalized_transition: Vec::new(),
                request_id: RequestId::from_bytes([seed.wrapping_add(7); 32]),
            })
            .unwrap()
        {
            OperationCommitOutcome::WorkingRecorded { head, .. } => head,
            OperationCommitOutcome::Conflict { .. } => panic!("unexpected local conflict"),
        };
        let request_id = RequestId::from_bytes([seed.wrapping_add(8); 32]);
        let request = BranchPushRequest {
            request_id,
            transfer_id: request_id,
            candidate_digest: layerfs_storage::BranchPushIdentityBuilder::new(request_id)
                .finish(accepted),
            expected: Some(expected),
            counters: SyncTransferCounters {
                unique_bytes: 13,
                resumed_bytes: 5,
                retransmitted_bytes: 2,
            },
        };
        assert_eq!(
            engine
                .product_record_replayed_branch_push(request, accepted)
                .unwrap(),
            BranchPushOutcome::DurablyAccepted {
                head: accepted,
                reconciled: false,
            }
        );
        Self {
            directory,
            database,
            engine,
            request,
            expected,
            accepted,
        }
    }

    fn receipt(&self) -> ReceiptRow {
        Connection::open(&self.database)
            .unwrap()
            .query_row(
                "SELECT durable_storage_id, direction, candidate_kind, candidate_id,
                        identity_version, transfer_id, candidate_digest,
                        expected_head_id, expected_generation, expected_root_id, result,
                        accepted_head_id, accepted_generation, accepted_root_id,
                        unique_bytes, resumed_bytes, retransmitted_bytes,
                        reconciliation_result
                 FROM layerfs_sync_receipts WHERE request_id = ?1",
                params![self.request.request_id.as_bytes()],
                |row| {
                    Ok(ReceiptRow {
                        durable_storage_id: row.get(0)?,
                        direction: row.get(1)?,
                        candidate_kind: row.get(2)?,
                        candidate_id: row.get(3)?,
                        identity_version: row.get(4)?,
                        transfer_id: row.get(5)?,
                        candidate_digest: row.get(6)?,
                        expected_head_id: row.get(7)?,
                        expected_generation: row.get(8)?,
                        expected_root_id: row.get(9)?,
                        result: row.get(10)?,
                        accepted_head_id: row.get(11)?,
                        accepted_generation: row.get(12)?,
                        accepted_root_id: row.get(13)?,
                        unique_bytes: row.get(14)?,
                        resumed_bytes: row.get(15)?,
                        retransmitted_bytes: row.get(16)?,
                        reconciliation_result: row.get(17)?,
                    })
                },
            )
            .unwrap()
    }

    fn update_receipt(&self, assignment: &str, value: &[u8]) {
        Connection::open(&self.database)
            .unwrap()
            .execute(
                &format!(
                    "UPDATE layerfs_sync_receipts SET {assignment} = ?1 WHERE request_id = ?2"
                ),
                params![value, self.request.request_id.as_bytes()],
            )
            .unwrap();
    }

    fn update_receipt_text(&self, assignment: &str, value: &str) {
        Connection::open(&self.database)
            .unwrap()
            .execute(
                &format!(
                    "UPDATE layerfs_sync_receipts SET {assignment} = ?1 WHERE request_id = ?2"
                ),
                params![value, self.request.request_id.as_bytes()],
            )
            .unwrap();
    }

    fn change_direction_to_fetch(&self) {
        Connection::open(&self.database)
            .unwrap()
            .execute(
                "UPDATE layerfs_sync_receipts
                 SET direction = 'fetch', identity_version = NULL,
                     transfer_id = NULL, candidate_digest = NULL
                 WHERE request_id = ?1",
                params![self.request.request_id.as_bytes()],
            )
            .unwrap();
    }

    fn update_durable_storage_id(&self, durable_storage_id: &[u8; 32]) {
        let connection = Connection::open(&self.database).unwrap();
        connection
            .execute(
                "INSERT INTO layerfs_durable_storages
                 (durable_storage_id, authenticated_at) VALUES (?1, 0)",
                params![durable_storage_id],
            )
            .unwrap();
        connection
            .execute(
                "UPDATE layerfs_sync_receipts SET durable_storage_id = ?1
                 WHERE request_id = ?2",
                params![durable_storage_id, self.request.request_id.as_bytes()],
            )
            .unwrap();
    }

    fn assert_replay_rejected(&self, request: BranchPushRequest, accepted: BranchHead) {
        let receipt = self.receipt();
        let head = self
            .engine
            .product_branch_head(self.accepted.branch_id)
            .unwrap();
        assert!(self
            .engine
            .product_record_replayed_branch_push(request, accepted)
            .is_err());
        assert_eq!(self.receipt(), receipt);
        assert_eq!(
            self.engine
                .product_branch_head(self.accepted.branch_id)
                .unwrap(),
            head
        );
    }

    fn assert_reconciliation_rejected(&self, request: BranchPushRequest, accepted: BranchHead) {
        let receipt = self.receipt();
        let head = self
            .engine
            .product_branch_head(self.accepted.branch_id)
            .unwrap();
        assert!(self
            .engine
            .product_reconcile_branch_push(request, accepted)
            .is_err());
        assert_eq!(self.receipt(), receipt);
        assert_eq!(
            self.engine
                .product_branch_head(self.accepted.branch_id)
                .unwrap(),
            head
        );
    }

    fn cleanup(self) {
        drop(self.engine);
        fs::remove_dir_all(self.directory).unwrap();
    }
}

#[test]
fn exact_replayed_push_returns_the_stored_reconciled_acceptance() {
    let fixture = ReplayFixture::new(0x31);
    assert_eq!(
        fixture
            .engine
            .product_record_replayed_branch_push(fixture.request, fixture.accepted)
            .unwrap(),
        BranchPushOutcome::DurablyAccepted {
            head: fixture.accepted,
            reconciled: true,
        }
    );
    assert_eq!(
        fixture
            .engine
            .product_reconcile_branch_push(fixture.request, fixture.accepted)
            .unwrap(),
        BranchPushOutcome::DurablyAccepted {
            head: fixture.accepted,
            reconciled: true,
        }
    );
    fixture.cleanup();
}

#[test]
fn replay_rejects_a_changed_expected_head_and_preserves_authority() {
    let fixture = ReplayFixture::new(0x41);
    let mut request = fixture.request;
    request.expected = Some(BranchHead {
        operation_version_id: Some(OperationVersionId::from_bytes([0xe1; 32])),
        ..fixture.expected
    });
    fixture.assert_replay_rejected(request, fixture.accepted);
    fixture.cleanup();
}

#[test]
fn replay_rejects_a_changed_expected_generation_and_preserves_authority() {
    let fixture = ReplayFixture::new(0x51);
    let mut request = fixture.request;
    request.expected = Some(BranchHead {
        generation: fixture.expected.generation + 1,
        ..fixture.expected
    });
    fixture.assert_replay_rejected(request, fixture.accepted);
    fixture.cleanup();
}

#[test]
fn replay_rejects_a_changed_expected_root_and_preserves_authority() {
    let fixture = ReplayFixture::new(0x52);
    let mut request = fixture.request;
    request.expected = Some(BranchHead {
        root: valid_empty_root_with_seed(&fixture.engine, [0xe2; 32]),
        ..fixture.expected
    });
    fixture.assert_replay_rejected(request, fixture.accepted);
    fixture.cleanup();
}

#[test]
fn replay_rejects_a_changed_candidate_and_preserves_authority() {
    let fixture = ReplayFixture::new(0x61);
    fixture.update_receipt("candidate_id", &[0xc1; 32]);
    fixture.assert_replay_rejected(fixture.request, fixture.accepted);
    fixture.cleanup();
}

#[test]
fn replay_rejects_a_changed_accepted_head_and_preserves_authority() {
    let fixture = ReplayFixture::new(0x71);
    let accepted = BranchHead {
        operation_version_id: Some(OperationVersionId::from_bytes([0xd1; 32])),
        ..fixture.accepted
    };
    fixture.assert_replay_rejected(fixture.request, accepted);
    fixture.cleanup();
}

#[test]
fn replay_rejects_a_changed_accepted_generation_and_preserves_authority() {
    let fixture = ReplayFixture::new(0x81);
    let accepted = BranchHead {
        generation: fixture.accepted.generation + 1,
        ..fixture.accepted
    };
    fixture.assert_replay_rejected(fixture.request, accepted);
    fixture.cleanup();
}

#[test]
fn replay_rejects_a_changed_accepted_root_and_preserves_authority() {
    let fixture = ReplayFixture::new(0x91);
    let accepted = BranchHead {
        root: valid_empty_root_with_seed(&fixture.engine, [0xd2; 32]),
        ..fixture.accepted
    };
    fixture.assert_replay_rejected(fixture.request, accepted);
    fixture.cleanup();
}

#[test]
fn replay_rejects_a_changed_direction_and_preserves_authority() {
    let fixture = ReplayFixture::new(0xa1);
    fixture.change_direction_to_fetch();
    fixture.assert_replay_rejected(fixture.request, fixture.accepted);
    fixture.cleanup();
}

#[test]
fn replay_rejects_changed_counters_and_preserves_authority() {
    let fixture = ReplayFixture::new(0xb1);
    let mut request = fixture.request;
    request.counters.unique_bytes += 1;
    fixture.assert_replay_rejected(request, fixture.accepted);
    fixture.cleanup();

    let fixture = ReplayFixture::new(0xc1);
    let mut request = fixture.request;
    request.counters.resumed_bytes += 1;
    fixture.assert_replay_rejected(request, fixture.accepted);
    fixture.cleanup();

    let fixture = ReplayFixture::new(0xd1);
    let mut request = fixture.request;
    request.counters.retransmitted_bytes += 1;
    fixture.assert_replay_rejected(request, fixture.accepted);
    fixture.cleanup();
}

#[test]
fn replay_rejects_changed_candidate_digest_or_transfer_and_preserves_authority() {
    let fixture = ReplayFixture::new(0xb2);
    let mut request = fixture.request;
    request.candidate_digest = [0xe4; 32];
    fixture.assert_replay_rejected(request, fixture.accepted);
    fixture.cleanup();

    let fixture = ReplayFixture::new(0xb3);
    let mut request = fixture.request;
    request.transfer_id = RequestId::from_bytes([0xe5; 32]);
    request.candidate_digest = layerfs_storage::BranchPushIdentityBuilder::new(request.transfer_id)
        .finish(fixture.accepted);
    fixture.assert_replay_rejected(request, fixture.accepted);
    fixture.cleanup();
}

#[test]
fn replay_rejects_changed_authority_kind_or_receipt_marker() {
    let fixture = ReplayFixture::new(0xc2);
    fixture.update_durable_storage_id(&[0xe2; 32]);
    fixture.assert_replay_rejected(fixture.request, fixture.accepted);
    fixture.cleanup();

    let fixture = ReplayFixture::new(0xc3);
    fixture.update_receipt_text("candidate_kind", "layer");
    fixture.assert_replay_rejected(fixture.request, fixture.accepted);
    fixture.cleanup();

    let fixture = ReplayFixture::new(0xc4);
    fixture.update_receipt_text("reconciliation_result", "unexpected");
    fixture.assert_replay_rejected(fixture.request, fixture.accepted);
    fixture.cleanup();
}

#[test]
fn reconciliation_rejects_changed_accepted_identity_and_preserves_authority() {
    let fixture = ReplayFixture::new(0xe1);
    let accepted = BranchHead {
        root: valid_empty_root_with_seed(&fixture.engine, [0xe3; 32]),
        ..fixture.accepted
    };
    fixture.assert_reconciliation_rejected(fixture.request, accepted);
    fixture.cleanup();
}
