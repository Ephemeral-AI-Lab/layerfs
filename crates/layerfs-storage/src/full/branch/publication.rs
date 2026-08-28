use crate::error::{map_sqlite_error, EngineError, EngineResult};
use crate::full::branch::read::{
    read_branch_ancestry, read_full_branch_head, BranchHead, VersionRef,
};
use crate::full::branch::transition::{insert_full_ordinary_operation, sql_u64, version_blob};
use crate::full::legacy_store::checked_add;
use crate::full::record_id::{bytes32, object_id, BranchId, OperationVersionId};
use crate::full::store::FullStorage;
use crate::full::transfer::batch::{
    branch_push_page_digest, BranchPushBundle, BranchPushIdentityBuilder, BranchPushOutcome,
    BranchPushRequest, SyncTransferCounters, BRANCH_PUSH_IDENTITY_VERSION,
};
use crate::full::transfer::custody::full_transaction;
use crate::full::transfer::page::read_staged_branch_push_pages;
use rusqlite::types::FromSql;
use rusqlite::{params, Connection, OptionalExtension, Row};

type HeadFields = (Option<Vec<u8>>, Option<i64>, Option<Vec<u8>>);
impl FullStorage {
    pub fn commit_verified_ordinary_branch_push(
        &self,
        request: BranchPushRequest,
        branch_id: BranchId,
    ) -> EngineResult<BranchPushOutcome> {
        self.require_authority()?;
        full_transaction(self, |connection| {
            let pages = read_staged_branch_push_pages(connection, request.transfer_id, branch_id)?;
            if validate_pages(request, &pages)? != branch_id {
                return Err(invalid("Full Branch Push BranchId"));
            }
            self.bump_durable_head_transaction()?;
            commit_pages(self, connection, request, &pages, branch_id)
        })
    }

    pub fn reconcile_verified_ordinary_branch_push(
        &self,
        request: BranchPushRequest,
        branch_id: BranchId,
    ) -> EngineResult<BranchPushOutcome> {
        self.require_authority()?;
        let connection = self.lock_connection()?;
        read_receipt(&connection, self.storage_id(), request, branch_id)?
            .ok_or(invalid("Full Branch Push receipt"))
    }
}

fn commit_pages(
    storage: &FullStorage,
    connection: &Connection,
    request: BranchPushRequest,
    pages: &[BranchPushBundle],
    branch_id: BranchId,
) -> EngineResult<BranchPushOutcome> {
    let authority = storage.storage_id();
    verify_staged_pages(connection, authority, request, pages, branch_id)?;
    if let Some(outcome) = read_receipt(connection, authority, request, branch_id)? {
        return Ok(outcome);
    }
    let actual = read_full_branch_head(connection, branch_id)?;
    if actual != request.expected {
        write(
            connection, authority, request, branch_id, actual, "conflict",
        )?;
        return Ok(BranchPushOutcome::Conflict { actual });
    }
    let first = &pages[0];
    validate_ancestry(connection, first)?;
    if actual.is_none() {
        insert_branch_base(connection, first)?;
    } else if read_branch_ancestry(connection, branch_id)? != Some(first.ancestry) {
        return Err(invalid("Full Branch ancestry changed"));
    }
    let genesis = BranchHead {
        branch_id,
        generation: 0,
        operation_version_id: None,
        root: first.ancestry.fork_root,
    };
    let mut prior = actual.unwrap_or(genesis);
    for page in pages {
        for operation in &page.operations {
            if operation.base != expected_base(first, prior)? {
                return Err(invalid("Full operation base"));
            }
            prior = insert_full_ordinary_operation(connection, branch_id, prior, operation)?;
        }
        if page.head != prior {
            return Err(invalid("Full Branch page head"));
        }
    }
    let initial = actual.unwrap_or(genesis);
    let changed = connection
        .execute(
            "UPDATE layerfs_branches SET generation = ?1, head_operation_version_id = ?2
             WHERE branch_id = ?3 AND generation = ?4 AND head_operation_version_id IS ?5 AND state = 'active'",
            params![
                sql_u64(prior.generation)?,
                version_blob(prior.operation_version_id),
                branch_id.as_bytes(),
                sql_u64(initial.generation)?,
                version_blob(initial.operation_version_id),
            ],
        )
        .map_err(map_sqlite_error)?;
    if changed != 1 {
        return Err(EngineError::PublicationConflict);
    }
    write(
        connection,
        authority,
        request,
        branch_id,
        Some(prior),
        "durably_accepted",
    )?;
    Ok(BranchPushOutcome::DurablyAccepted {
        head: prior,
        reconciled: false,
    })
}

fn validate_pages(
    request: BranchPushRequest,
    pages: &[BranchPushBundle],
) -> EngineResult<BranchId> {
    let first = pages.first().ok_or(invalid("Full Branch Push pages"))?;
    let branch_id = first.head.branch_id;
    let expected = request.expected;
    if expected.is_some_and(|head| head.branch_id != branch_id) {
        return Err(invalid("Full Branch expected head"));
    }
    let mut base = expected;
    let mut operation_count = 0_usize;
    for (index, page) in pages.iter().enumerate() {
        operation_count = operation_count
            .checked_add(page.operations.len())
            .ok_or(EngineError::CounterOverflow)?;
        if page.head.branch_id != branch_id
            || page.base != base
            || page.ancestry != first.ancestry
            || page.name != first.name
            || page.origin_stack != first.origin_stack
            || page.complete != (index + 1 == pages.len())
            || !page.child_merges.is_empty()
            || !page.rollbacks.is_empty()
            || !page.dependencies.is_empty()
            || !page.origin_stack.layers.is_empty()
            || !page.origin_stack.transitions.is_empty()
        {
            return Err(invalid("Full ordinary Branch pages"));
        }
        base = Some(page.head);
    }
    if operation_count == 0 {
        return Err(invalid("Full ordinary Branch history"));
    }
    Ok(branch_id)
}

fn verify_staged_pages(
    connection: &Connection,
    authority: [u8; 32],
    request: BranchPushRequest,
    pages: &[BranchPushBundle],
    branch_id: BranchId,
) -> EngineResult<()> {
    let mut statement = connection
        .prepare(
            "SELECT page_sequence, data_request_id, bundle, identity_version, page_digest,
                    unique_bytes, resumed_bytes, retransmitted_bytes, branch_id, origin_authority_storage_id FROM layerfs_branch_push_pages
             WHERE transfer_id = ?1 ORDER BY page_sequence",
        )
        .map_err(map_sqlite_error)?;
    let mut rows = statement
        .query(params![request.transfer_id.as_bytes()])
        .map_err(map_sqlite_error)?;
    let mut identity = BranchPushIdentityBuilder::new(request.transfer_id);
    let mut observed = SyncTransferCounters::default();
    for (sequence, page) in pages.iter().enumerate() {
        let row = rows
            .next()
            .map_err(map_sqlite_error)?
            .ok_or(invalid("Full staged Push pages"))?;
        let sequence = u64::try_from(sequence).map_err(|_| EngineError::CounterOverflow)?;
        let data_request = bytes32(&column::<Vec<u8>>(row, 1)?, "RequestId")?;
        let encoded = serde_json::to_vec(page)
            .map_err(|_| EngineError::InvalidRecord("Push page encoding"))?;
        let counters = SyncTransferCounters {
            unique_bytes: row_u64(column(row, 5)?)?,
            resumed_bytes: row_u64(column(row, 6)?)?,
            retransmitted_bytes: row_u64(column(row, 7)?)?,
        };
        let digest = branch_push_page_digest(
            request.transfer_id,
            sequence,
            crate::full::record_id::RequestId(data_request),
            branch_id,
            &encoded,
            counters,
        );
        if row_u64(column(row, 0)?)? != sequence
            || column::<Vec<u8>>(row, 2)? != encoded
            || row_u64(column(row, 3)?)? != BRANCH_PUSH_IDENTITY_VERSION
            || column::<Vec<u8>>(row, 4)?.as_slice() != digest
            || column::<Vec<u8>>(row, 8)?.as_slice() != branch_id.as_bytes()
            || column::<Vec<u8>>(row, 9)?.as_slice() != authority
        {
            return Err(invalid("Full staged Push identity"));
        }
        identity.absorb_page(sequence, digest)?;
        add_counters(&mut observed, counters)?;
    }
    if rows.next().map_err(map_sqlite_error)?.is_some()
        || observed != request.counters
        || identity.finish(pages.last().unwrap().head) != request.candidate_digest
    {
        return Err(invalid("Full staged Push candidate"));
    }
    Ok(())
}

fn validate_ancestry(connection: &Connection, bundle: &BranchPushBundle) -> EngineResult<()> {
    let ancestry = bundle.ancestry;
    let origin_root = connection
        .query_row(
            "SELECT result_root_id FROM layerfs_layers WHERE layer_stack_id = ?1 AND layer_id = ?2 AND state = 'accepted'",
            params![ancestry.origin_layer_stack_id.0, ancestry.origin_layer_id.0],
            |row| row.get::<_, Vec<u8>>(0),
        )
        .optional()
        .map_err(map_sqlite_error)?
        .ok_or(invalid("Full Branch origin Layer"))?;
    match (
        ancestry.immediate_parent_branch_id,
        ancestry.fork_operation_id,
        ancestry.fork_operation_version_id,
    ) {
        (None, None, None)
            if ancestry.depth == 0 && ancestry.fork_root == object_id(&origin_root)? =>
        {
            Ok(())
        }
        (Some(parent), Some(operation), Some(version)) if ancestry.depth > 0 => {
            let parent_ancestry =
                read_branch_ancestry(connection, parent)?.ok_or(invalid("Full parent Branch"))?;
            let valid = connection
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM layerfs_operation_versions
                     WHERE branch_id = ?1 AND operation_version_id = ?2
                       AND result_root_id = ?3 AND created_by_kind = 'operation'
                       AND operation_id = ?4)",
                    params![
                        parent.as_bytes(),
                        version.as_bytes(),
                        ancestry.fork_root.as_bytes(),
                        operation.as_bytes()
                    ],
                    |row| row.get::<_, bool>(0),
                )
                .map_err(map_sqlite_error)?;
            if !valid
                || parent_ancestry.origin_layer_stack_id != ancestry.origin_layer_stack_id
                || parent_ancestry.origin_layer_id != ancestry.origin_layer_id
                || parent_ancestry.depth.checked_add(1) != Some(ancestry.depth)
            {
                return Err(invalid("Full child Branch ancestry"));
            }
            Ok(())
        }
        _ => Err(invalid("Full Branch ancestry")),
    }
}

fn insert_branch_base(connection: &Connection, bundle: &BranchPushBundle) -> EngineResult<()> {
    let ancestry = bundle.ancestry;
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
                ancestry.immediate_parent_branch_id.map(|id| id.0.to_vec()),
                ancestry.fork_operation_id.map(|id| id.0.to_vec()),
                ancestry.fork_operation_version_id.map(|id| id.0.to_vec()),
                ancestry.fork_root.as_bytes(),
                ancestry.origin_layer_stack_id.as_bytes(),
                ancestry.origin_layer_id.as_bytes(),
                sql_u64(ancestry.depth)?,
            ],
        )
        .map_err(map_sqlite_error)?;
    Ok(())
}

fn expected_base(bundle: &BranchPushBundle, head: BranchHead) -> EngineResult<VersionRef> {
    match head.operation_version_id {
        Some(operation_version_id) => Ok(VersionRef::OperationVersion {
            branch_id: head.branch_id,
            operation_version_id,
            root: head.root,
        }),
        None if head.generation == 0 => Ok(VersionRef::Layer {
            layer_stack_id: bundle.ancestry.origin_layer_stack_id,
            layer_id: bundle.ancestry.origin_layer_id,
            root: head.root,
        }),
        None => Err(invalid("Full Branch head")),
    }
}

fn read_receipt(
    connection: &Connection,
    authority: [u8; 32],
    request: BranchPushRequest,
    branch_id: BranchId,
) -> EngineResult<Option<BranchPushOutcome>> {
    let expected = request.expected;
    if expected.is_some_and(|head| head.branch_id != branch_id) {
        return Err(invalid("Full Branch expected head"));
    }
    let (expected_version, expected_generation, expected_root) = head_fields(expected)?;
    let row = connection
        .query_row(
            "SELECT result, decided_head_present, decided_head_id, decided_generation, decided_root_id, CASE WHEN result != 'durably_accepted' THEN 1 ELSE EXISTS(SELECT 1 FROM layerfs_operation_versions v
                      JOIN layerfs_branch_transitions t ON t.branch_id = v.branch_id AND t.after_operation_version_id = v.operation_version_id
                      WHERE v.branch_id = candidate_id AND v.operation_version_id = decided_head_id AND v.result_root_id = decided_root_id
                        AND t.after_generation = decided_generation) END
             FROM layerfs_sync_receipts WHERE request_id = ?1 AND authority_storage_id = ?2
              AND direction = 'push' AND candidate_kind = 'branch' AND candidate_id = ?3
              AND identity_version = ?4 AND transfer_id = ?5 AND candidate_digest = ?6
              AND expected_head_id IS ?7 AND expected_generation IS ?8 AND expected_root_id IS ?9
              AND unique_bytes = ?10 AND resumed_bytes = ?11 AND retransmitted_bytes = ?12
              AND reconciliation_result = 'exact'",
            params![
                request.request_id.as_bytes(),
                authority.as_slice(),
                branch_id.as_bytes(),
                sql_u64(BRANCH_PUSH_IDENTITY_VERSION)?,
                request.transfer_id.as_bytes(),
                request.candidate_digest.as_slice(),
                expected_version,
                expected_generation,
                expected_root,
                sql_u64(request.counters.unique_bytes)?,
                sql_u64(request.counters.resumed_bytes)?,
                sql_u64(request.counters.retransmitted_bytes)?,
            ],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, bool>(1)?,
                    row.get::<_, Option<Vec<u8>>>(2)?,
                    row.get::<_, Option<i64>>(3)?,
                    row.get::<_, Option<Vec<u8>>>(4)?,
                    row.get::<_, bool>(5)?,
                ))
            },
        )
        .optional()
        .map_err(map_sqlite_error)?;
    let Some((result, present, version, generation, root, history_present)) = row else {
        let reused = connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM layerfs_sync_receipts WHERE request_id = ?1)",
                params![request.request_id.as_bytes()],
                |row| row.get::<_, bool>(0),
            )
            .map_err(map_sqlite_error)?;
        return if reused {
            Err(invalid("Full Branch Push request identity"))
        } else {
            Ok(None)
        };
    };
    let decided = decode_decided(branch_id, present, version, generation, root)?;
    match result.as_str() {
        "durably_accepted" => {
            let head = decided.ok_or(EngineError::InvalidRecord("Full accepted receipt"))?;
            if !history_present {
                return Err(invalid("Full accepted receipt history"));
            }
            Ok(Some(BranchPushOutcome::DurablyAccepted {
                head,
                reconciled: true,
            }))
        }
        "conflict" => Ok(Some(BranchPushOutcome::Conflict { actual: decided })),
        _ => Err(invalid("Full Branch Push receipt result")),
    }
}

fn write(
    connection: &Connection,
    authority: [u8; 32],
    request: BranchPushRequest,
    branch_id: BranchId,
    decided: Option<BranchHead>,
    result: &str,
) -> EngineResult<()> {
    let (expected_version, expected_generation, expected_root) = head_fields(request.expected)?;
    let (decided_version, decided_generation, decided_root) = head_fields(decided)?;
    connection
        .execute(
            "INSERT INTO layerfs_sync_receipts
             (request_id, authority_storage_id, direction, candidate_kind, candidate_id, identity_version,
              transfer_id, candidate_digest, expected_head_id, expected_generation, expected_root_id,
              decided_head_present, decided_head_id, decided_generation, decided_root_id, result,
              unique_bytes, resumed_bytes, retransmitted_bytes, reconciliation_result)
             VALUES (?1, ?2, 'push', 'branch', ?3, ?4, ?5, ?6, ?7, ?8, ?9,
                     ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, 'exact')",
            params![
                request.request_id.as_bytes(),
                authority.as_slice(),
                branch_id.as_bytes(),
                sql_u64(BRANCH_PUSH_IDENTITY_VERSION)?,
                request.transfer_id.as_bytes(),
                request.candidate_digest.as_slice(),
                expected_version,
                expected_generation,
                expected_root,
                i64::from(decided.is_some()),
                decided_version,
                decided_generation,
                decided_root,
                result,
                sql_u64(request.counters.unique_bytes)?,
                sql_u64(request.counters.resumed_bytes)?,
                sql_u64(request.counters.retransmitted_bytes)?,
            ],
        )
        .map_err(map_sqlite_error)?;
    Ok(())
}

fn decode_decided(
    branch_id: BranchId,
    present: bool,
    version: Option<Vec<u8>>,
    generation: Option<i64>,
    root: Option<Vec<u8>>,
) -> EngineResult<Option<BranchHead>> {
    if !present {
        return if version.is_none() && generation.is_none() && root.is_none() {
            Ok(None)
        } else {
            Err(invalid("Full absent receipt head"))
        };
    }
    let generation = generation
        .and_then(|value| u64::try_from(value).ok())
        .ok_or(invalid("Full receipt generation"))?;
    let operation_version_id = version
        .as_deref()
        .map(|value| bytes32(value, "OperationVersionId").map(OperationVersionId))
        .transpose()?;
    if (generation == 0) != operation_version_id.is_none() {
        return Err(invalid("Full receipt head"));
    }
    Ok(Some(BranchHead {
        branch_id,
        generation,
        operation_version_id,
        root: object_id(&root.ok_or(invalid("Full receipt root"))?)?,
    }))
}

fn head_fields(head: Option<BranchHead>) -> EngineResult<HeadFields> {
    Ok(match head {
        Some(head) => (
            version_blob(head.operation_version_id),
            Some(sql_u64(head.generation)?),
            Some(head.root.as_bytes().to_vec()),
        ),
        None => (None, None, None),
    })
}
fn add_counters(total: &mut SyncTransferCounters, value: SyncTransferCounters) -> EngineResult<()> {
    checked_add(&mut total.unique_bytes, value.unique_bytes)?;
    checked_add(&mut total.resumed_bytes, value.resumed_bytes)?;
    checked_add(&mut total.retransmitted_bytes, value.retransmitted_bytes)?;
    Ok(())
}

fn row_u64(value: i64) -> EngineResult<u64> {
    u64::try_from(value).map_err(|_| EngineError::InvalidRecord("Full Push counter"))
}
fn column<T: FromSql>(row: &Row<'_>, index: usize) -> EngineResult<T> {
    row.get(index).map_err(map_sqlite_error)
}
fn invalid(name: &'static str) -> EngineError {
    EngineError::InvalidRecord(name)
}
