//! Authoritative fetch, Push, and reconciliation receipts.

use crate::error::{map_sqlite_error, EngineError, EngineResult};
use crate::full::branch::import::insert_verified_fetch_rows;
use crate::full::branch::read::{
    branch_contains_exact_historical_version, read_branch_head, BranchHead,
};
use crate::full::layer_stack::read::LayerStackHead;
use crate::full::legacy_store::{
    begin_product_transaction, checked_add, commit_product_request, commit_product_state,
    rollback_product_transaction, Engine,
};
use crate::full::record_id::{bytes32, object_id, BranchId, OperationVersionId, RequestId};
use crate::full::transfer::batch::{
    BranchPushBundle, BranchPushIdentityBuilder, BranchPushOutcome, BranchPushRequest,
    SyncTransferCounters, VerifiedFetchRequest, BRANCH_PUSH_IDENTITY_VERSION,
};
use crate::full::transfer::custody::connection_store_id;
use crate::sqlite::ConnectionGuard;
use crate::working::lease::unix_seconds;
use layerfs_core::ObjectId;
use rusqlite::{params, Connection, OptionalExtension};
use std::sync::atomic::Ordering;

pub(crate) fn commit_verified_fetch(
    engine: &Engine,
    connection: &mut ConnectionGuard<'_>,
    request: VerifiedFetchRequest,
    head: BranchHead,
    stack_head: LayerStackHead,
) -> EngineResult<bool> {
    if engine.fetch_boundary_failure.swap(false, Ordering::AcqRel) {
        return Err(EngineError::InvalidRecord(
            "injected Fetch history/tracking boundary failure",
        ));
    }
    connection
        .execute(
            "DELETE FROM layerfs_sync_object_pins
             WHERE owner_request_id = ?1 AND direction = 'fetch'",
            params![request.request_id.as_bytes()],
        )
        .map_err(map_sqlite_error)?;
    connection
        .execute(
            "DELETE FROM layerfs_transfer_state
             WHERE request_id = ?1 AND direction = 'fetch'",
            params![request.request_id.as_bytes()],
        )
        .map_err(map_sqlite_error)?;
    let incumbent = insert_verified_fetch_rows(engine, connection, request, head, stack_head)?;
    let reconciled = commit_product_request(
        engine,
        connection,
        "layerfs_sync_receipts",
        request.request_id,
    )?;
    Ok(incumbent || reconciled)
}

pub(crate) fn commit_partial_fetch(
    engine: &Engine,
    connection: &mut ConnectionGuard<'_>,
    request: VerifiedFetchRequest,
    head: BranchHead,
) -> EngineResult<bool> {
    connection
        .execute(
            "DELETE FROM layerfs_sync_object_pins
             WHERE owner_request_id = ?1 AND direction = 'fetch'",
            params![request.request_id.as_bytes()],
        )
        .map_err(map_sqlite_error)?;
    connection
        .execute(
            "DELETE FROM layerfs_transfer_state
             WHERE request_id = ?1 AND direction = 'fetch'",
            params![request.request_id.as_bytes()],
        )
        .map_err(map_sqlite_error)?;
    commit_product_state(
        engine,
        connection,
        "SELECT EXISTS(SELECT 1 FROM layerfs_fetch_staging_heads
         WHERE target_kind = 'branch' AND target_id = ?1)",
        head.branch_id.as_bytes(),
    )
}

pub(crate) fn finish_fetch_target(
    connection: &Connection,
    target_kind: &str,
    target_id: &[u8; 32],
) -> EngineResult<()> {
    connection
        .execute(
            "DELETE FROM layerfs_fetch_staging_heads
             WHERE target_kind = ?1 AND target_id = ?2",
            params![target_kind, target_id],
        )
        .map_err(map_sqlite_error)?;
    Ok(())
}

pub(crate) fn finish_fetch_staging(
    connection: &Connection,
    branch_id: BranchId,
) -> EngineResult<()> {
    finish_fetch_target(connection, "branch", branch_id.as_bytes())
}

#[derive(Debug)]
struct StoredPushReceipt {
    durable_storage_id: [u8; 32],
    direction: String,
    candidate_kind: String,
    candidate_id: BranchId,
    identity_version: Option<u64>,
    transfer_id: Option<RequestId>,
    candidate_digest: Option<[u8; 32]>,
    expected_head_id: Option<OperationVersionId>,
    expected_generation: Option<u64>,
    expected_root_id: Option<ObjectId>,
    result: String,
    accepted_head_id: Option<OperationVersionId>,
    accepted_generation: Option<u64>,
    accepted_root_id: Option<ObjectId>,
    counters: SyncTransferCounters,
    reconciliation_result: Option<String>,
}

fn optional_u64(value: Option<i64>, field: &'static str) -> EngineResult<Option<u64>> {
    value
        .map(|value| u64::try_from(value).map_err(|_| EngineError::InvalidRecord(field)))
        .transpose()
}

fn load_push_receipt(
    connection: &Connection,
    request_id: RequestId,
) -> EngineResult<Option<StoredPushReceipt>> {
    let row = connection
        .query_row(
            "SELECT durable_storage_id, direction, candidate_kind, candidate_id,
                    identity_version, transfer_id, candidate_digest,
                    expected_head_id, expected_generation, expected_root_id, result,
                    accepted_head_id, accepted_generation, accepted_root_id,
                    unique_bytes, resumed_bytes, retransmitted_bytes,
                    reconciliation_result
             FROM layerfs_sync_receipts WHERE request_id = ?1",
            params![request_id.as_bytes()],
            |row| {
                Ok((
                    row.get::<_, Vec<u8>>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Vec<u8>>(3)?,
                    row.get::<_, Option<i64>>(4)?,
                    row.get::<_, Option<Vec<u8>>>(5)?,
                    row.get::<_, Option<Vec<u8>>>(6)?,
                    row.get::<_, Option<Vec<u8>>>(7)?,
                    row.get::<_, Option<i64>>(8)?,
                    row.get::<_, Option<Vec<u8>>>(9)?,
                    row.get::<_, String>(10)?,
                    row.get::<_, Option<Vec<u8>>>(11)?,
                    row.get::<_, Option<i64>>(12)?,
                    row.get::<_, Option<Vec<u8>>>(13)?,
                    row.get::<_, i64>(14)?,
                    row.get::<_, i64>(15)?,
                    row.get::<_, i64>(16)?,
                    row.get::<_, Option<String>>(17)?,
                ))
            },
        )
        .optional()
        .map_err(map_sqlite_error)?;
    let Some(row) = row else {
        return Ok(None);
    };
    Ok(Some(StoredPushReceipt {
        durable_storage_id: bytes32(&row.0, "DurableStorageId")?,
        direction: row.1,
        candidate_kind: row.2,
        candidate_id: BranchId(bytes32(&row.3, "BranchId")?),
        identity_version: optional_u64(row.4, "Push identity version")?,
        transfer_id: row
            .5
            .map(|id| bytes32(&id, "Push transfer ID").map(RequestId))
            .transpose()?,
        candidate_digest: row
            .6
            .map(|digest| bytes32(&digest, "Push candidate digest"))
            .transpose()?,
        expected_head_id: row
            .7
            .map(|id| bytes32(&id, "OperationVersionId").map(OperationVersionId))
            .transpose()?,
        expected_generation: optional_u64(row.8, "Branch generation")?,
        expected_root_id: row.9.map(|root| object_id(&root)).transpose()?,
        result: row.10,
        accepted_head_id: row
            .11
            .map(|id| bytes32(&id, "OperationVersionId").map(OperationVersionId))
            .transpose()?,
        accepted_generation: optional_u64(row.12, "Branch generation")?,
        accepted_root_id: row.13.map(|root| object_id(&root)).transpose()?,
        counters: SyncTransferCounters {
            unique_bytes: u64::try_from(row.14)
                .map_err(|_| EngineError::InvalidRecord("Push unique bytes"))?,
            resumed_bytes: u64::try_from(row.15)
                .map_err(|_| EngineError::InvalidRecord("Push resumed bytes"))?,
            retransmitted_bytes: u64::try_from(row.16)
                .map_err(|_| EngineError::InvalidRecord("Push retransmitted bytes"))?,
        },
        reconciliation_result: row.17,
    }))
}

fn validate_existing_push_receipt(
    connection: &Connection,
    request: BranchPushRequest,
    branch_id: BranchId,
    claimed_accepted: Option<BranchHead>,
    require_accepted: bool,
) -> EngineResult<Option<BranchPushOutcome>> {
    let Some(receipt) = load_push_receipt(connection, request.request_id)? else {
        return Ok(None);
    };
    if receipt.durable_storage_id != connection_store_id(connection)?
        || receipt.direction != "push"
        || receipt.candidate_kind != "branch"
        || receipt.candidate_id != branch_id
        || receipt.identity_version != Some(BRANCH_PUSH_IDENTITY_VERSION)
        || receipt.transfer_id != Some(request.transfer_id)
        || receipt.candidate_digest != Some(request.candidate_digest)
        || receipt.expected_head_id != request.expected.and_then(|head| head.operation_version_id)
        || receipt.expected_generation != request.expected.map(|head| head.generation)
        || receipt.expected_root_id != request.expected.map(|head| head.root)
        || receipt.counters != request.counters
        || !matches!(
            receipt.reconciliation_result.as_deref(),
            Some("exact" | "ordered_replay")
        )
    {
        return Err(EngineError::InvalidRecord("Push request identity conflict"));
    }
    match receipt.result.as_str() {
        "durably_accepted" => {
            let head = BranchHead {
                branch_id,
                generation: receipt
                    .accepted_generation
                    .ok_or(EngineError::InvalidRecord("Push accepted generation"))?,
                operation_version_id: receipt.accepted_head_id,
                root: receipt
                    .accepted_root_id
                    .ok_or(EngineError::InvalidRecord("Push accepted root"))?,
            };
            if claimed_accepted.is_some_and(|accepted| accepted != head)
                || require_accepted && claimed_accepted.is_none()
                || !branch_contains_exact_historical_version(connection, head)?
            {
                return Err(EngineError::InvalidRecord(
                    "Push accepted identity conflict",
                ));
            }
            Ok(Some(BranchPushOutcome::DurablyAccepted {
                head,
                reconciled: true,
            }))
        }
        "conflict"
            if !require_accepted
                && receipt.accepted_head_id.is_none()
                && receipt.accepted_generation.is_none()
                && receipt.accepted_root_id.is_none() =>
        {
            Ok(Some(BranchPushOutcome::Conflict {
                actual: read_branch_head(connection, branch_id)?,
            }))
        }
        _ => Err(EngineError::InvalidRecord("Push receipt result")),
    }
}

pub(crate) fn read_push_receipt(
    connection: &Connection,
    request: BranchPushRequest,
    bundle: &BranchPushBundle,
) -> EngineResult<Option<BranchPushOutcome>> {
    validate_existing_push_receipt(
        connection,
        request,
        bundle.head.branch_id,
        Some(bundle.head),
        false,
    )
}

fn persist_push_receipt(
    engine: &Engine,
    connection: &Connection,
    request: BranchPushRequest,
    branch_id: BranchId,
    accepted: Option<BranchHead>,
    reconciliation_result: &str,
) -> EngineResult<()> {
    let result = if accepted.is_some() {
        "durably_accepted"
    } else {
        "conflict"
    };
    connection
        .execute(
            "INSERT INTO layerfs_durable_storages
             (durable_storage_id, authenticated_at) VALUES (?1, ?2)
             ON CONFLICT(durable_storage_id) DO UPDATE
             SET authenticated_at = excluded.authenticated_at",
            params![engine.store_id_cached().as_slice(), unix_seconds()?],
        )
        .map_err(map_sqlite_error)?;
    connection
        .execute(
            "INSERT INTO layerfs_sync_receipts
             (request_id, durable_storage_id, direction, candidate_kind,
              candidate_id, identity_version, transfer_id, candidate_digest,
              expected_head_id, expected_generation, expected_root_id, result,
              accepted_head_id, accepted_generation, accepted_root_id,
              unique_bytes, resumed_bytes, retransmitted_bytes,
              reconciliation_result)
             VALUES (?1, ?2, 'push', 'branch', ?3, 1, ?4, ?5, ?6, ?7, ?8, ?9,
                     ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
            params![
                request.request_id.as_bytes(),
                engine.store_id_cached().as_slice(),
                branch_id.as_bytes(),
                request.transfer_id.as_bytes(),
                request.candidate_digest.as_slice(),
                request
                    .expected
                    .and_then(|head| head.operation_version_id)
                    .map(|id| id.as_bytes().as_slice().to_vec()),
                request
                    .expected
                    .map(|head| i64::try_from(head.generation))
                    .transpose()
                    .map_err(|_| EngineError::CounterOverflow)?,
                request
                    .expected
                    .map(|head| head.root.as_bytes().as_slice().to_vec()),
                result,
                accepted
                    .and_then(|head| head.operation_version_id)
                    .map(|id| id.as_bytes().as_slice().to_vec()),
                accepted
                    .map(|head| i64::try_from(head.generation))
                    .transpose()
                    .map_err(|_| EngineError::CounterOverflow)?,
                accepted.map(|head| head.root.as_bytes().as_slice().to_vec()),
                i64::try_from(request.counters.unique_bytes)
                    .map_err(|_| EngineError::CounterOverflow)?,
                i64::try_from(request.counters.resumed_bytes)
                    .map_err(|_| EngineError::CounterOverflow)?,
                i64::try_from(request.counters.retransmitted_bytes)
                    .map_err(|_| EngineError::CounterOverflow)?,
                reconciliation_result,
            ],
        )
        .map_err(map_sqlite_error)?;
    Ok(())
}

pub(crate) fn insert_push_receipt(
    engine: &Engine,
    connection: &Connection,
    request: BranchPushRequest,
    bundle: &BranchPushBundle,
    result: &str,
) -> EngineResult<()> {
    if !matches!(result, "durably_accepted" | "conflict") {
        return Err(EngineError::InvalidRecord("Push result"));
    }
    persist_push_receipt(
        engine,
        connection,
        request,
        bundle.head.branch_id,
        (result == "durably_accepted").then_some(bundle.head),
        "exact",
    )
}

impl Engine {
    pub fn product_reconcile_branch_push(
        &self,
        request: BranchPushRequest,
        accepted: BranchHead,
    ) -> EngineResult<BranchPushOutcome> {
        let connection = self.lock_connection()?;
        validate_existing_push_receipt(
            &connection,
            request,
            accepted.branch_id,
            Some(accepted),
            true,
        )?
        .ok_or(EngineError::InvalidRecord("Push receipt"))
    }

    pub fn product_record_replayed_branch_push(
        &self,
        request: BranchPushRequest,
        accepted: BranchHead,
    ) -> EngineResult<BranchPushOutcome> {
        if BranchPushIdentityBuilder::new(request.transfer_id).finish(accepted)
            != request.candidate_digest
        {
            return Err(EngineError::InvalidRecord("Push candidate digest"));
        }
        let mut connection = begin_product_transaction(self)?;
        let result = (|| {
            if let Some(outcome) = validate_existing_push_receipt(
                &connection,
                request,
                accepted.branch_id,
                Some(accepted),
                true,
            )? {
                self.commit_dispatch
                    .rollback(&connection)
                    .map_err(map_sqlite_error)?;
                connection.transaction = false;
                self.bump(|counters| checked_add(&mut counters.transactions_rolled_back, 1))?;
                return Ok(outcome);
            }
            if read_branch_head(&connection, accepted.branch_id)? != Some(accepted) {
                return Err(EngineError::PublicationConflict);
            }
            persist_push_receipt(
                self,
                &connection,
                request,
                accepted.branch_id,
                Some(accepted),
                "ordered_replay",
            )?;
            commit_product_request(
                self,
                &mut connection,
                "layerfs_sync_receipts",
                request.request_id,
            )?;
            Ok(BranchPushOutcome::DurablyAccepted {
                head: accepted,
                reconciled: false,
            })
        })();
        rollback_product_transaction(self, &mut connection, &result);
        result
    }
}
