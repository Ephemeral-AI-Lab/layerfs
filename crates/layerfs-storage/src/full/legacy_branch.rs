//! Transitional legacy-full staged Branch publication transaction.

use crate::error::{map_sqlite_error, EngineError, EngineResult};
use crate::full::branch::import::verify_staged_child_merges;
use crate::full::branch::read::{read_branch_ancestry, read_branch_head, BranchHead};
use crate::full::branch::transition::{insert_pushed_branch_rollback, insert_pushed_child_merge};
use crate::full::layer_stack::read::{read_layer_root, read_layer_stack_head};
use crate::full::lease::insert_branch_origin_lease;
use crate::full::legacy_store::{
    begin_product_transaction, checked_add, commit_product_request, rollback_product_transaction,
    Engine,
};
use crate::full::operation::record::insert_pushed_operation;
use crate::full::receipt::{insert_push_receipt, read_push_receipt};
use crate::full::record_id::{bytes32, object_id, BranchId, RequestId};
use crate::full::transfer::batch::validate_staged_push_page;
use crate::full::transfer::batch::{
    branch_push_page_digest, BranchPushBundle, BranchPushIdentityBuilder, BranchPushOutcome,
    BranchPushRequest, SyncTransferCounters, BRANCH_PUSH_IDENTITY_VERSION,
};
use crate::full::transfer::custody::release_staged_push_pins;
use rusqlite::{params, Connection, OptionalExtension};

fn verify_staged_push_identity(
    connection: &Connection,
    request: BranchPushRequest,
    branch_id: BranchId,
) -> EngineResult<(i64, BranchPushBundle)> {
    let (page_count, maximum) = connection
        .query_row(
            "SELECT COUNT(*), MAX(page_sequence)
             FROM layerfs_branch_push_pages WHERE transfer_id = ?1 AND branch_id = ?2",
            params![request.transfer_id.as_bytes(), branch_id.as_bytes()],
            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, Option<i64>>(1)?)),
        )
        .map_err(map_sqlite_error)?;
    let maximum = maximum.ok_or(EngineError::InvalidRecord("Push pages"))?;
    if page_count != maximum.checked_add(1).ok_or(EngineError::CounterOverflow)? {
        return Err(EngineError::InvalidRecord("Push page sequence"));
    }
    let mut identity = BranchPushIdentityBuilder::new(request.transfer_id);
    let mut observed = SyncTransferCounters::default();
    let mut final_bundle = None;
    for sequence in 0..=maximum {
        let row = connection
            .query_row(
                "SELECT data_request_id, bundle, identity_version, page_digest,
                        unique_bytes, resumed_bytes, retransmitted_bytes
                 FROM layerfs_branch_push_pages
                 WHERE transfer_id = ?1 AND branch_id = ?2 AND page_sequence = ?3",
                params![
                    request.transfer_id.as_bytes(),
                    branch_id.as_bytes(),
                    sequence
                ],
                |row| {
                    Ok((
                        row.get::<_, Vec<u8>>(0)?,
                        row.get::<_, Vec<u8>>(1)?,
                        row.get::<_, i64>(2)?,
                        row.get::<_, Vec<u8>>(3)?,
                        row.get::<_, i64>(4)?,
                        row.get::<_, i64>(5)?,
                        row.get::<_, i64>(6)?,
                    ))
                },
            )
            .map_err(map_sqlite_error)?;
        let page_sequence = u64::try_from(sequence)
            .map_err(|_| EngineError::InvalidRecord("Push page sequence"))?;
        let data_request_id = RequestId(bytes32(&row.0, "Push data request ID")?);
        let counters = SyncTransferCounters {
            unique_bytes: u64::try_from(row.4)
                .map_err(|_| EngineError::InvalidRecord("Push unique bytes"))?,
            resumed_bytes: u64::try_from(row.5)
                .map_err(|_| EngineError::InvalidRecord("Push resumed bytes"))?,
            retransmitted_bytes: u64::try_from(row.6)
                .map_err(|_| EngineError::InvalidRecord("Push retransmitted bytes"))?,
        };
        let digest = branch_push_page_digest(
            request.transfer_id,
            page_sequence,
            data_request_id,
            branch_id,
            &row.1,
            counters,
        );
        if u64::try_from(row.2).ok() != Some(BRANCH_PUSH_IDENTITY_VERSION)
            || row.3.as_slice() != digest
        {
            return Err(EngineError::InvalidRecord("Push page digest"));
        }
        identity.absorb_page(page_sequence, digest)?;
        checked_add(&mut observed.unique_bytes, counters.unique_bytes)?;
        checked_add(&mut observed.resumed_bytes, counters.resumed_bytes)?;
        checked_add(
            &mut observed.retransmitted_bytes,
            counters.retransmitted_bytes,
        )?;
        let bundle: BranchPushBundle = serde_json::from_slice(&row.1)
            .map_err(|_| EngineError::InvalidRecord("Push page encoding"))?;
        if sequence == maximum {
            final_bundle = Some(bundle);
        }
    }
    let final_bundle = final_bundle.ok_or(EngineError::InvalidRecord("Push pages"))?;
    if observed != request.counters
        || !final_bundle.complete
        || identity.finish(final_bundle.head) != request.candidate_digest
    {
        return Err(EngineError::InvalidRecord("Push candidate identity"));
    }
    Ok((maximum, final_bundle))
}

impl Engine {
    pub fn product_commit_staged_branch_push(
        &self,
        request: BranchPushRequest,
        branch_id: BranchId,
    ) -> EngineResult<BranchPushOutcome> {
        {
            let connection = self.lock_connection()?;
            verify_staged_push_identity(&connection, request, branch_id)?;
        }
        verify_staged_child_merges(self, request.transfer_id, branch_id)?;
        let mut connection = begin_product_transaction(self)?;
        let result = (|| {
            let (maximum, final_bundle) =
                verify_staged_push_identity(&connection, request, branch_id)?;
            if let Some(outcome) = read_push_receipt(&connection, request, &final_bundle)? {
                release_staged_push_pins(&connection, request.transfer_id)?;
                connection
                    .execute(
                        "DELETE FROM layerfs_branch_push_pages WHERE transfer_id = ?1",
                        params![request.transfer_id.as_bytes()],
                    )
                    .map_err(map_sqlite_error)?;
                let _ = commit_product_request(
                    self,
                    &mut connection,
                    "layerfs_sync_receipts",
                    request.request_id,
                )?;
                return Ok(outcome);
            }
            self.bump(|counters| checked_add(&mut counters.durable_head_transactions, 1))?;
            let actual = read_branch_head(&connection, branch_id)?;
            if actual != request.expected {
                insert_push_receipt(self, &connection, request, &final_bundle, "conflict")?;
                release_staged_push_pins(&connection, request.transfer_id)?;
                connection
                    .execute(
                        "DELETE FROM layerfs_branch_push_pages WHERE transfer_id = ?1",
                        params![request.transfer_id.as_bytes()],
                    )
                    .map_err(map_sqlite_error)?;
                let _ = commit_product_request(
                    self,
                    &mut connection,
                    "layerfs_sync_receipts",
                    request.request_id,
                )?;
                return Ok(BranchPushOutcome::Conflict { actual });
            }

            let mut prior_version = actual.and_then(|head| head.operation_version_id);
            let mut prior_root = actual.map(|head| head.root);
            let mut prior_generation = actual.map_or(0, |head| head.generation);
            let mut ancestry = None;
            let mut branch_name = None;
            let mut final_head = None;
            for sequence in 0..=maximum {
                let encoded = connection
                    .query_row(
                        "SELECT bundle FROM layerfs_branch_push_pages
                         WHERE transfer_id = ?1 AND branch_id = ?2 AND page_sequence = ?3",
                        params![
                            request.transfer_id.as_bytes(),
                            branch_id.as_bytes(),
                            sequence
                        ],
                        |row| row.get::<_, Vec<u8>>(0),
                    )
                    .map_err(map_sqlite_error)?;
                let bundle: BranchPushBundle = serde_json::from_slice(&encoded)
                    .map_err(|_| EngineError::InvalidRecord("Push page encoding"))?;
                validate_staged_push_page(&bundle)?;
                if bundle.head.branch_id != branch_id
                    || bundle.base != final_head.or(request.expected)
                    || ancestry.is_some_and(|value| value != bundle.ancestry)
                    || branch_name
                        .as_ref()
                        .is_some_and(|value| value != &bundle.name)
                    || bundle.complete != (sequence == maximum)
                {
                    return Err(EngineError::InvalidRecord("Push page chain"));
                }
                if ancestry.is_none() {
                    validate_push_ancestry(&connection, &bundle)?;
                    ancestry = Some(bundle.ancestry);
                    branch_name = Some(bundle.name.clone());
                    if actual.is_none() {
                        insert_branch_base(&connection, &bundle)?;
                        prior_root = Some(bundle.ancestry.fork_root);
                    } else if read_branch_ancestry(&connection, branch_id)? != Some(bundle.ancestry)
                    {
                        return Err(EngineError::InvalidRecord("Push Branch ancestry changed"));
                    }
                }
                let mut history = Vec::with_capacity(
                    bundle
                        .operations
                        .len()
                        .checked_add(bundle.child_merges.len())
                        .and_then(|count| count.checked_add(bundle.rollbacks.len()))
                        .ok_or(EngineError::CounterOverflow)?,
                );
                history.extend(
                    bundle
                        .operations
                        .iter()
                        .enumerate()
                        .map(|(index, record)| (record.after_generation, 0_u8, index)),
                );
                history.extend(
                    bundle
                        .child_merges
                        .iter()
                        .enumerate()
                        .map(|(index, record)| (record.after_generation, 1_u8, index)),
                );
                history.extend(
                    bundle
                        .rollbacks
                        .iter()
                        .enumerate()
                        .map(|(index, record)| (record.after_generation, 2_u8, index)),
                );
                history.sort_unstable();
                for (_, kind, index) in history {
                    let next = match kind {
                        0 => insert_pushed_operation(
                            self,
                            &connection,
                            &bundle,
                            &bundle.operations[index],
                            prior_version,
                            prior_root.ok_or(EngineError::InvalidRecord("Push Branch base"))?,
                            prior_generation,
                        )?,
                        1 => insert_pushed_child_merge(
                            self,
                            &connection,
                            branch_id,
                            &bundle.child_merges[index],
                            prior_version,
                            prior_root.ok_or(EngineError::InvalidRecord("Push Branch base"))?,
                            prior_generation,
                            None,
                        )?,
                        2 => insert_pushed_branch_rollback(
                            &connection,
                            branch_id,
                            &bundle.rollbacks[index],
                            prior_version,
                            prior_generation,
                        )?,
                        _ => unreachable!(),
                    };
                    prior_version = Some(next.0);
                    prior_root = Some(next.1);
                    prior_generation = next.2;
                }
                if prior_version != bundle.head.operation_version_id
                    || prior_root != Some(bundle.head.root)
                    || prior_generation != bundle.head.generation
                {
                    return Err(EngineError::InvalidRecord("Push page head"));
                }
                final_head = Some(bundle.head);
            }
            let final_head = final_head.ok_or(EngineError::InvalidRecord("Push pages"))?;
            let initial = actual.unwrap_or(BranchHead {
                branch_id,
                generation: 0,
                operation_version_id: None,
                root: ancestry
                    .ok_or(EngineError::InvalidRecord("Push ancestry"))?
                    .fork_root,
            });
            if final_head != initial {
                let changed = connection
                    .execute(
                        "UPDATE layerfs_branches
                         SET generation = ?1, head_operation_version_id = ?2
                         WHERE branch_id = ?3 AND generation = ?4
                           AND head_operation_version_id IS ?5 AND state = 'active'",
                        params![
                            i64::try_from(final_head.generation)
                                .map_err(|_| EngineError::CounterOverflow)?,
                            final_head
                                .operation_version_id
                                .map(|id| id.as_bytes().as_slice().to_vec()),
                            branch_id.as_bytes(),
                            i64::try_from(initial.generation)
                                .map_err(|_| EngineError::CounterOverflow)?,
                            initial
                                .operation_version_id
                                .map(|id| id.as_bytes().as_slice().to_vec()),
                        ],
                    )
                    .map_err(map_sqlite_error)?;
                if changed != 1 {
                    return Err(EngineError::PublicationConflict);
                }
            }
            if actual.is_none() {
                insert_branch_origin_lease(&connection, &final_bundle)?;
            }
            insert_push_receipt(
                self,
                &connection,
                request,
                &final_bundle,
                "durably_accepted",
            )?;
            release_staged_push_pins(&connection, request.transfer_id)?;
            connection
                .execute(
                    "DELETE FROM layerfs_branch_push_pages WHERE transfer_id = ?1",
                    params![request.transfer_id.as_bytes()],
                )
                .map_err(map_sqlite_error)?;
            let reconciled = commit_product_request(
                self,
                &mut connection,
                "layerfs_sync_receipts",
                request.request_id,
            )?;
            Ok(BranchPushOutcome::DurablyAccepted {
                head: final_head,
                reconciled,
            })
        })();
        rollback_product_transaction(self, &mut connection, &result);
        result
    }
}
pub(crate) fn validate_push_ancestry(
    connection: &Connection,
    bundle: &BranchPushBundle,
) -> EngineResult<()> {
    if read_layer_stack_head(connection, bundle.ancestry.origin_layer_stack_id)?.is_none() {
        return Err(EngineError::InvalidRecord("Push origin LayerStack"));
    }
    let origin_root = read_layer_root(
        connection,
        bundle.ancestry.origin_layer_stack_id,
        bundle.ancestry.origin_layer_id,
    )?
    .ok_or(EngineError::InvalidRecord("Push origin Layer"))?;
    match (
        bundle.ancestry.immediate_parent_branch_id,
        bundle.ancestry.fork_operation_id,
        bundle.ancestry.fork_operation_version_id,
    ) {
        (None, None, None)
            if bundle.ancestry.depth == 0 && bundle.ancestry.fork_root == origin_root =>
        {
            Ok(())
        }
        (Some(parent), Some(operation), Some(version)) if bundle.ancestry.depth > 0 => {
            let parent_ancestry = read_branch_ancestry(connection, parent)?
                .ok_or(EngineError::InvalidRecord("Push parent Branch"))?;
            let fork = connection
                .query_row(
                    "SELECT root_id, created_by_kind, created_by_operation_id
                     FROM layerfs_operation_versions
                     WHERE branch_id = ?1 AND operation_version_id = ?2",
                    params![parent.as_bytes(), version.as_bytes()],
                    |row| {
                        Ok((
                            row.get::<_, Vec<u8>>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, Option<Vec<u8>>>(2)?,
                        ))
                    },
                )
                .optional()
                .map_err(map_sqlite_error)?
                .ok_or(EngineError::InvalidRecord("Push child origin"))?;
            if object_id(&fork.0)? != bundle.ancestry.fork_root
                || fork.1 != "operation"
                || fork.2.as_deref() != Some(operation.as_bytes())
                || parent_ancestry.origin_layer_stack_id != bundle.ancestry.origin_layer_stack_id
                || parent_ancestry.origin_layer_id != bundle.ancestry.origin_layer_id
                || parent_ancestry.depth.checked_add(1) != Some(bundle.ancestry.depth)
            {
                return Err(EngineError::InvalidRecord("Push child ancestry"));
            }
            Ok(())
        }
        _ => Err(EngineError::InvalidRecord("Push Branch ancestry")),
    }
}

pub(crate) fn insert_branch_base(
    connection: &Connection,
    bundle: &BranchPushBundle,
) -> EngineResult<()> {
    connection
        .execute(
            "INSERT INTO layerfs_branches
             (branch_id, name, immediate_parent_branch_id, fork_operation_id,
              fork_operation_version_id, fork_root_id, origin_layer_stack_id,
              origin_layer_id, depth, generation, head_operation_version_id, state)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 0, NULL, 'active')",
            params![
                bundle.head.branch_id.as_bytes(),
                bundle.name.as_deref(),
                bundle
                    .ancestry
                    .immediate_parent_branch_id
                    .map(|id| id.as_bytes().as_slice().to_vec()),
                bundle
                    .ancestry
                    .fork_operation_id
                    .map(|id| id.as_bytes().as_slice().to_vec()),
                bundle
                    .ancestry
                    .fork_operation_version_id
                    .map(|id| id.as_bytes().as_slice().to_vec()),
                bundle.ancestry.fork_root.as_bytes(),
                bundle.ancestry.origin_layer_stack_id.as_bytes(),
                bundle.ancestry.origin_layer_id.as_bytes(),
                i64::try_from(bundle.ancestry.depth).map_err(|_| EngineError::CounterOverflow)?,
            ],
        )
        .map_err(map_sqlite_error)?;
    Ok(())
}

pub(crate) fn insert_branch_snapshot(
    connection: &Connection,
    bundle: &BranchPushBundle,
) -> EngineResult<()> {
    connection
        .execute(
            "INSERT INTO layerfs_branches
             (branch_id, name, immediate_parent_branch_id, fork_operation_id,
              fork_operation_version_id, fork_root_id, origin_layer_stack_id,
              origin_layer_id, depth, generation, head_operation_version_id, state)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, 'active')",
            params![
                bundle.head.branch_id.as_bytes(),
                bundle.name.as_deref(),
                bundle
                    .ancestry
                    .immediate_parent_branch_id
                    .map(|id| id.as_bytes().as_slice().to_vec()),
                bundle
                    .ancestry
                    .fork_operation_id
                    .map(|id| id.as_bytes().as_slice().to_vec()),
                bundle
                    .ancestry
                    .fork_operation_version_id
                    .map(|id| id.as_bytes().as_slice().to_vec()),
                bundle.ancestry.fork_root.as_bytes(),
                bundle.ancestry.origin_layer_stack_id.as_bytes(),
                bundle.ancestry.origin_layer_id.as_bytes(),
                i64::try_from(bundle.ancestry.depth).map_err(|_| EngineError::CounterOverflow)?,
                i64::try_from(bundle.head.generation).map_err(|_| EngineError::CounterOverflow)?,
                bundle
                    .head
                    .operation_version_id
                    .map(|id| id.as_bytes().as_slice().to_vec()),
            ],
        )
        .map_err(map_sqlite_error)?;
    Ok(())
}
