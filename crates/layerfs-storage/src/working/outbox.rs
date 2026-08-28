//! Working Push outbox and reconciliation state.

use crate::error::{map_sqlite_error, EngineError, EngineResult};
use crate::full::branch::read::{branch_contains_exact_version, read_branch_ancestry, BranchHead};
use crate::full::legacy_store::{
    begin_product_transaction, commit_product_state, rollback_product_transaction, Engine,
};
use crate::full::record_id::{bytes32, object_id, BranchId, OperationVersionId, RequestId};
use crate::full::transfer::batch::{
    BranchPushRequest, SyncTransferCounters, BRANCH_PUSH_IDENTITY_VERSION,
};
use crate::working::lease::unix_seconds;
use rusqlite::Connection;
use rusqlite::{params, OptionalExtension};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PushOutboxEntry {
    pub head: BranchHead,
    pub state: String,
    pub request: Option<BranchPushRequest>,
}

struct StoredPushOutbox {
    durable_storage_id: [u8; 32],
    expected: Option<BranchHead>,
    entry: PushOutboxEntry,
}

fn load_push_outbox(
    connection: &Connection,
    request_id: RequestId,
) -> EngineResult<Option<StoredPushOutbox>> {
    let row = connection
        .query_row(
            "SELECT durable_storage_id, branch_id, operation_version_id,
                    accepted_generation, accepted_root_id,
                    expected_head_id, expected_durable_generation, expected_root_id,
                    identity_version, transfer_id, candidate_digest,
                    unique_bytes, resumed_bytes, retransmitted_bytes, state
             FROM layerfs_push_outbox WHERE request_id = ?1",
            params![request_id.as_bytes()],
            |row| {
                Ok((
                    row.get::<_, Vec<u8>>(0)?,
                    row.get::<_, Vec<u8>>(1)?,
                    row.get::<_, Option<Vec<u8>>>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, Vec<u8>>(4)?,
                    row.get::<_, Option<Vec<u8>>>(5)?,
                    row.get::<_, Option<i64>>(6)?,
                    row.get::<_, Option<Vec<u8>>>(7)?,
                    row.get::<_, Option<i64>>(8)?,
                    row.get::<_, Option<Vec<u8>>>(9)?,
                    row.get::<_, Option<Vec<u8>>>(10)?,
                    row.get::<_, Option<i64>>(11)?,
                    row.get::<_, Option<i64>>(12)?,
                    row.get::<_, Option<i64>>(13)?,
                    row.get::<_, String>(14)?,
                ))
            },
        )
        .optional()
        .map_err(map_sqlite_error)?;
    let Some(row) = row else {
        return Ok(None);
    };
    let branch_id = BranchId(bytes32(&row.1, "BranchId")?);
    let head = BranchHead {
        branch_id,
        generation: u64::try_from(row.3)
            .map_err(|_| EngineError::InvalidRecord("Branch generation"))?,
        operation_version_id: row
            .2
            .map(|version| bytes32(&version, "OperationVersionId").map(OperationVersionId))
            .transpose()?,
        root: object_id(&row.4)?,
    };
    let expected = match row.6 {
        Some(generation) => Some(BranchHead {
            branch_id,
            generation: u64::try_from(generation)
                .map_err(|_| EngineError::InvalidRecord("Branch generation"))?,
            operation_version_id: row
                .5
                .map(|version| bytes32(&version, "OperationVersionId").map(OperationVersionId))
                .transpose()?,
            root: object_id(
                row.7
                    .as_deref()
                    .ok_or(EngineError::InvalidRecord("Push expected root"))?,
            )?,
        }),
        None if row.5.is_none() && row.7.is_none() => None,
        None => return Err(EngineError::InvalidRecord("Push expected head")),
    };
    let request = match row.8 {
        Some(version) if u64::try_from(version).ok() == Some(BRANCH_PUSH_IDENTITY_VERSION) => {
            Some(BranchPushRequest {
                request_id,
                transfer_id: RequestId(bytes32(
                    row.9
                        .as_deref()
                        .ok_or(EngineError::InvalidRecord("Push transfer ID"))?,
                    "Push transfer ID",
                )?),
                candidate_digest: bytes32(
                    row.10
                        .as_deref()
                        .ok_or(EngineError::InvalidRecord("Push candidate digest"))?,
                    "Push candidate digest",
                )?,
                expected,
                counters: SyncTransferCounters {
                    unique_bytes: u64::try_from(
                        row.11
                            .ok_or(EngineError::InvalidRecord("Push unique bytes"))?,
                    )
                    .map_err(|_| EngineError::InvalidRecord("Push unique bytes"))?,
                    resumed_bytes: u64::try_from(
                        row.12
                            .ok_or(EngineError::InvalidRecord("Push resumed bytes"))?,
                    )
                    .map_err(|_| EngineError::InvalidRecord("Push resumed bytes"))?,
                    retransmitted_bytes: u64::try_from(
                        row.13
                            .ok_or(EngineError::InvalidRecord("Push retransmitted bytes"))?,
                    )
                    .map_err(|_| EngineError::InvalidRecord("Push retransmitted bytes"))?,
                },
            })
        }
        None if row.9.is_none()
            && row.10.is_none()
            && row.11.is_none()
            && row.12.is_none()
            && row.13.is_none() =>
        {
            None
        }
        _ => return Err(EngineError::InvalidRecord("Push outbox identity")),
    };
    Ok(Some(StoredPushOutbox {
        durable_storage_id: bytes32(&row.0, "DurableStorageId")?,
        expected,
        entry: PushOutboxEntry {
            head,
            state: row.14,
            request,
        },
    }))
}

impl Engine {
    pub fn product_record_push_outbox(
        &self,
        request_id: RequestId,
        durable_storage_id: [u8; 32],
        head: BranchHead,
        expected: Option<BranchHead>,
        request: Option<BranchPushRequest>,
        state: &str,
    ) -> EngineResult<bool> {
        if !matches!(
            state,
            "selected" | "transferring" | "transferred" | "accepted" | "conflict" | "indeterminate"
        ) {
            return Err(EngineError::InvalidRecord("Push outbox state"));
        }
        if request
            .is_some_and(|request| request.request_id != request_id || request.expected != expected)
            || matches!(
                state,
                "transferred" | "accepted" | "conflict" | "indeterminate"
            ) && request.is_none()
        {
            return Err(EngineError::InvalidRecord("Push outbox identity"));
        }
        {
            let connection = self.lock_connection()?;
            let retained = if head.generation == 0 && head.operation_version_id.is_none() {
                read_branch_ancestry(&connection, head.branch_id)?
                    .is_some_and(|ancestry| ancestry.fork_root == head.root)
            } else {
                branch_contains_exact_version(&connection, head)?
            };
            if !retained {
                return Err(EngineError::PublicationConflict);
            }
        }
        let mut connection = begin_product_transaction(self)?;
        let result = (|| {
            connection
                .execute(
                    "INSERT INTO layerfs_durable_storages
                     (durable_storage_id, authenticated_at) VALUES (?1, ?2)
                     ON CONFLICT(durable_storage_id) DO UPDATE
                     SET authenticated_at = excluded.authenticated_at",
                    params![durable_storage_id.as_slice(), unix_seconds()?],
                )
                .map_err(map_sqlite_error)?;
            let incumbent = load_push_outbox(&connection, request_id)?;
            let version = head
                .operation_version_id
                .map(|id| id.as_bytes().as_slice().to_vec());
            let expected_version = expected
                .and_then(|head| head.operation_version_id)
                .map(|id| id.as_bytes().as_slice().to_vec());
            let expected_generation = expected
                .map(|head| i64::try_from(head.generation))
                .transpose()
                .map_err(|_| EngineError::CounterOverflow)?;
            let expected_root = expected.map(|head| head.root.as_bytes().as_slice().to_vec());
            if let Some(incumbent) = incumbent {
                if incumbent.durable_storage_id != durable_storage_id
                    || incumbent.entry.head != head
                    || incumbent.expected != expected
                    || matches!((incumbent.entry.request, request), (Some(old), Some(new)) if old != new)
                {
                    return Err(EngineError::InvalidRecord("Push outbox request conflict"));
                }
                if matches!(incumbent.entry.state.as_str(), "accepted" | "conflict") {
                    if state == incumbent.entry.state || !matches!(state, "accepted" | "conflict") {
                        return Ok(true);
                    }
                    return Err(EngineError::InvalidRecord("Push outbox terminal conflict"));
                }
                let (transfer_id, candidate_digest, unique, resumed, retransmitted) = request
                    .map(|request| {
                        (
                            Some(request.transfer_id.as_bytes().as_slice().to_vec()),
                            Some(request.candidate_digest.to_vec()),
                            Some(i64::try_from(request.counters.unique_bytes)),
                            Some(i64::try_from(request.counters.resumed_bytes)),
                            Some(i64::try_from(request.counters.retransmitted_bytes)),
                        )
                    })
                    .unwrap_or((None, None, None, None, None));
                connection
                    .execute(
                        "UPDATE layerfs_push_outbox
                         SET state = ?1,
                             identity_version = COALESCE(identity_version, ?2),
                             transfer_id = COALESCE(transfer_id, ?3),
                             candidate_digest = COALESCE(candidate_digest, ?4),
                             unique_bytes = COALESCE(unique_bytes, ?5),
                             resumed_bytes = COALESCE(resumed_bytes, ?6),
                             retransmitted_bytes = COALESCE(retransmitted_bytes, ?7)
                         WHERE request_id = ?8",
                        params![
                            state,
                            request
                                .map(|_| i64::try_from(BRANCH_PUSH_IDENTITY_VERSION))
                                .transpose()
                                .map_err(|_| EngineError::CounterOverflow)?,
                            transfer_id,
                            candidate_digest,
                            unique
                                .transpose()
                                .map_err(|_| EngineError::CounterOverflow)?,
                            resumed
                                .transpose()
                                .map_err(|_| EngineError::CounterOverflow)?,
                            retransmitted
                                .transpose()
                                .map_err(|_| EngineError::CounterOverflow)?,
                            request_id.as_bytes(),
                        ],
                    )
                    .map_err(map_sqlite_error)?;
            } else {
                connection
                    .execute(
                        "INSERT INTO layerfs_push_outbox
                         (request_id, durable_storage_id, branch_id,
                          operation_version_id, accepted_generation, accepted_root_id,
                          expected_head_id, expected_durable_generation, expected_root_id,
                          identity_version, transfer_id, candidate_digest,
                          unique_bytes, resumed_bytes, retransmitted_bytes, state)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9,
                                 ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
                        params![
                            request_id.as_bytes(),
                            durable_storage_id.as_slice(),
                            head.branch_id.as_bytes(),
                            version,
                            i64::try_from(head.generation)
                                .map_err(|_| EngineError::CounterOverflow)?,
                            head.root.as_bytes(),
                            expected_version,
                            expected_generation,
                            expected_root,
                            request
                                .map(|_| i64::try_from(BRANCH_PUSH_IDENTITY_VERSION))
                                .transpose()
                                .map_err(|_| EngineError::CounterOverflow)?,
                            request.map(|request| request.transfer_id.as_bytes().to_vec()),
                            request.map(|request| request.candidate_digest.to_vec()),
                            request
                                .map(|request| i64::try_from(request.counters.unique_bytes))
                                .transpose()
                                .map_err(|_| EngineError::CounterOverflow)?,
                            request
                                .map(|request| i64::try_from(request.counters.resumed_bytes))
                                .transpose()
                                .map_err(|_| EngineError::CounterOverflow)?,
                            request
                                .map(|request| i64::try_from(request.counters.retransmitted_bytes))
                                .transpose()
                                .map_err(|_| EngineError::CounterOverflow)?,
                            state,
                        ],
                    )
                    .map_err(map_sqlite_error)?;
            }
            let reconciliation = format!(
                "SELECT EXISTS(SELECT 1 FROM layerfs_push_outbox \
                 WHERE request_id = ?1 AND state = '{state}')"
            );
            commit_product_state(
                self,
                &mut connection,
                &reconciliation,
                request_id.as_bytes(),
            )
        })();
        rollback_product_transaction(self, &mut connection, &result);
        result
    }

    pub fn product_push_outbox_state(&self, request_id: RequestId) -> EngineResult<Option<String>> {
        self.lock_connection()?
            .query_row(
                "SELECT state FROM layerfs_push_outbox WHERE request_id = ?1",
                params![request_id.as_bytes()],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(map_sqlite_error)
    }

    pub fn product_push_outbox_entry(
        &self,
        request_id: RequestId,
    ) -> EngineResult<Option<PushOutboxEntry>> {
        let connection = self.lock_connection()?;
        Ok(load_push_outbox(&connection, request_id)?.map(|stored| stored.entry))
    }
}
