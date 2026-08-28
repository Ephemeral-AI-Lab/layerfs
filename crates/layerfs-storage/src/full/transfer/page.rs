//! Bounded incoming Branch history pages.

use super::batch::{
    branch_push_page_digest, validate_staged_push_page, BranchPushBundle, SyncTransferCounters,
    BRANCH_PUSH_IDENTITY_VERSION, MAX_PUSH_OPERATION_RECORDS,
};
use super::custody::{delete_sync_custody, full_transaction};
use crate::error::{map_sqlite_error, EngineError, EngineResult};
use crate::full::legacy_store::{
    begin_product_transaction, checked_add, commit_product_state, rollback_product_transaction,
    Engine,
};
use crate::full::record_id::{bytes32, derive_id, BranchId, RequestId};
use crate::working::lease::unix_seconds;
use crate::FullStorage;
use rusqlite::{params, OptionalExtension};

impl Engine {
    pub fn product_stage_branch_push_page(
        &self,
        transfer_id: RequestId,
        page_sequence: u64,
        data_request_id: RequestId,
        bundle: &BranchPushBundle,
        counters: SyncTransferCounters,
    ) -> EngineResult<()> {
        validate_staged_push_page(bundle)?;
        let encoded = serde_json::to_vec(bundle)
            .map_err(|_| EngineError::InvalidRecord("Push page encoding"))?;
        if encoded.len() > 1024 * 1024 {
            return Err(EngineError::InvalidRecord("Push page resource bound"));
        }
        let page_digest = branch_push_page_digest(
            transfer_id,
            page_sequence,
            data_request_id,
            bundle.head.branch_id,
            &encoded,
            counters,
        );
        let page_id = derive_id(
            b"branch-push-page",
            &[transfer_id.as_bytes(), &page_sequence.to_be_bytes()],
        );
        let mut connection = begin_product_transaction(self)?;
        let result = (|| {
            let observed_unique_bytes = connection
                .query_row(
                    "SELECT COALESCE(SUM(o.canonical_length), 0)
                     FROM layerfs_sync_object_pins p
                     JOIN layerfs_objects o ON o.object_id = p.object_id
                     WHERE p.owner_request_id = ?1 AND p.request_id = ?2
                       AND p.direction = 'push'",
                    params![transfer_id.as_bytes(), data_request_id.as_bytes()],
                    |row| row.get::<_, i64>(0),
                )
                .map_err(map_sqlite_error)?;
            if u64::try_from(observed_unique_bytes).ok() != Some(counters.unique_bytes) {
                return Err(EngineError::InvalidRecord(
                    "Push page receiver-observed bytes",
                ));
            }
            let incumbent = connection
                .query_row(
                    "SELECT transfer_id, page_sequence, data_request_id, branch_id, bundle,
                            identity_version, page_digest,
                            unique_bytes, resumed_bytes, retransmitted_bytes
                     FROM layerfs_branch_push_pages WHERE page_id = ?1",
                    params![page_id.as_slice()],
                    |row| {
                        Ok((
                            row.get::<_, Vec<u8>>(0)?,
                            row.get::<_, i64>(1)?,
                            row.get::<_, Vec<u8>>(2)?,
                            row.get::<_, Vec<u8>>(3)?,
                            row.get::<_, Vec<u8>>(4)?,
                            row.get::<_, i64>(5)?,
                            row.get::<_, Vec<u8>>(6)?,
                            row.get::<_, i64>(7)?,
                            row.get::<_, i64>(8)?,
                            row.get::<_, i64>(9)?,
                        ))
                    },
                )
                .optional()
                .map_err(map_sqlite_error)?;
            if let Some(incumbent) = incumbent {
                if incumbent.0.as_slice() != transfer_id.as_bytes()
                    || u64::try_from(incumbent.1).ok() != Some(page_sequence)
                    || incumbent.2.as_slice() != data_request_id.as_bytes()
                    || incumbent.3.as_slice() != bundle.head.branch_id.as_bytes()
                    || incumbent.4 != encoded
                    || u64::try_from(incumbent.5).ok() != Some(BRANCH_PUSH_IDENTITY_VERSION)
                    || incumbent.6.as_slice() != page_digest
                    || u64::try_from(incumbent.7).ok() != Some(counters.unique_bytes)
                    || u64::try_from(incumbent.8).ok() != Some(counters.resumed_bytes)
                    || u64::try_from(incumbent.9).ok() != Some(counters.retransmitted_bytes)
                {
                    return Err(EngineError::InvalidRecord("Push page identity conflict"));
                }
            } else {
                connection
                    .execute(
                        "INSERT INTO layerfs_branch_push_pages
                         (page_id, transfer_id, page_sequence, data_request_id, branch_id, bundle,
                          identity_version, page_digest,
                          unique_bytes, resumed_bytes, retransmitted_bytes, created_at)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, 1, ?7, ?8, ?9, ?10, ?11)",
                        params![
                            page_id.as_slice(),
                            transfer_id.as_bytes(),
                            i64::try_from(page_sequence)
                                .map_err(|_| EngineError::CounterOverflow)?,
                            data_request_id.as_bytes(),
                            bundle.head.branch_id.as_bytes(),
                            encoded,
                            page_digest.as_slice(),
                            i64::try_from(counters.unique_bytes)
                                .map_err(|_| EngineError::CounterOverflow)?,
                            i64::try_from(counters.resumed_bytes)
                                .map_err(|_| EngineError::CounterOverflow)?,
                            i64::try_from(counters.retransmitted_bytes)
                                .map_err(|_| EngineError::CounterOverflow)?,
                            unix_seconds()?,
                        ],
                    )
                    .map_err(map_sqlite_error)?;
            }
            commit_product_state(
                self,
                &mut connection,
                "SELECT EXISTS(SELECT 1 FROM layerfs_branch_push_pages WHERE page_id = ?1)",
                &page_id,
            )?;
            Ok(())
        })();
        rollback_product_transaction(self, &mut connection, &result);
        result
    }

    pub fn product_abort_sync_transfer(
        &self,
        owner_request_id: RequestId,
        direction: &str,
    ) -> EngineResult<u64> {
        if !matches!(direction, "fetch" | "push") {
            return Err(EngineError::InvalidRecord("sync direction"));
        }
        let mut connection = begin_product_transaction(self)?;
        let result = delete_sync_custody(self, &mut connection, owner_request_id, direction);
        rollback_product_transaction(self, &mut connection, &result);
        result
    }

    pub fn product_reap_one_abandoned_sync(
        &self,
        older_than_unix_seconds: i64,
    ) -> EngineResult<Option<(RequestId, String, u64)>> {
        let mut connection = begin_product_transaction(self)?;
        let result = (|| {
            let owner = connection
                .query_row(
                    "SELECT owner_request_id, direction FROM (
                         SELECT owner_request_id, direction, created_at
                         FROM layerfs_sync_object_pins
                         UNION ALL
                         SELECT owner_request_id, direction, created_at
                         FROM layerfs_sync_batch_receipts
                         UNION ALL
                         SELECT transfer_id, 'push', created_at
                         FROM layerfs_branch_push_pages
                         UNION ALL
                         SELECT closure_id, 'closure', created_at
                         FROM layerfs_fetch_closure_items)
                     WHERE created_at < ?1
                     ORDER BY created_at, owner_request_id LIMIT 1",
                    params![older_than_unix_seconds],
                    |row| Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, String>(1)?)),
                )
                .optional()
                .map_err(map_sqlite_error)?;
            let Some((owner, direction)) = owner else {
                self.commit_dispatch
                    .rollback(&connection)
                    .map_err(map_sqlite_error)?;
                connection.transaction = false;
                self.bump(|counters| checked_add(&mut counters.transactions_rolled_back, 1))?;
                return Ok(None);
            };
            let owner = RequestId(bytes32(&owner, "RequestId")?);
            let rows = if direction == "closure" {
                let rows = connection
                    .execute(
                        "DELETE FROM layerfs_fetch_closure_items WHERE closure_id = ?1",
                        params![owner.as_bytes()],
                    )
                    .map_err(map_sqlite_error)?;
                commit_product_state(
                    self,
                    &mut connection,
                    "SELECT NOT EXISTS(SELECT 1 FROM layerfs_fetch_closure_items
                     WHERE closure_id = ?1)",
                    owner.as_bytes(),
                )?;
                u64::try_from(rows).map_err(|_| EngineError::CounterOverflow)?
            } else {
                delete_sync_custody(self, &mut connection, owner, &direction)?
            };
            Ok(Some((owner, direction, rows)))
        })();
        rollback_product_transaction(self, &mut connection, &result);
        result
    }

    pub fn product_sync_custody_rows(
        &self,
        owner_request_id: RequestId,
        direction: &str,
    ) -> EngineResult<u64> {
        if !matches!(direction, "fetch" | "push") {
            return Err(EngineError::InvalidRecord("sync direction"));
        }
        let connection = self.lock_connection()?;
        let rows = connection
            .query_row(
                "SELECT
                     (SELECT COUNT(*) FROM layerfs_sync_object_pins
                      WHERE owner_request_id = ?1 AND direction = ?2)
                   + (SELECT COUNT(*) FROM layerfs_sync_batch_receipts
                      WHERE owner_request_id = ?1 AND direction = ?2)
                   + (SELECT COUNT(*) FROM layerfs_branch_push_pages
                      WHERE transfer_id = ?1 AND ?2 = 'push')",
                params![owner_request_id.as_bytes(), direction],
                |row| row.get::<_, i64>(0),
            )
            .map_err(map_sqlite_error)?;
        u64::try_from(rows).map_err(|_| EngineError::CounterOverflow)
    }
}

impl FullStorage {
    pub fn stage_verified_branch_push_page(
        &self,
        transfer_id: RequestId,
        page_sequence: u64,
        data_request_id: RequestId,
        bundle: &BranchPushBundle,
        counters: SyncTransferCounters,
    ) -> EngineResult<()> {
        self.require_authority()?;
        validate_staged_push_page(bundle)?;
        let encoded = serde_json::to_vec(bundle)
            .map_err(|_| EngineError::InvalidRecord("Push page encoding"))?;
        if encoded.len() > 1024 * 1024 {
            return Err(EngineError::InvalidRecord("Push page resource bound"));
        }
        let page_digest = branch_push_page_digest(
            transfer_id,
            page_sequence,
            data_request_id,
            bundle.head.branch_id,
            &encoded,
            counters,
        );
        let page_id = derive_id(
            b"branch-push-page",
            &[transfer_id.as_bytes(), &page_sequence.to_be_bytes()],
        );
        let origin = bundle.origin_stack.head;
        let verification = derive_id(
            b"layerfs.full.origin-verification.v1",
            &[
                transfer_id.as_bytes(),
                &page_sequence.to_be_bytes(),
                origin.layer_id.as_bytes(),
            ],
        );
        let pin = derive_id(
            b"layerfs.full.origin-pin.v1",
            &[transfer_id.as_bytes(), origin.layer_id.as_bytes()],
        );
        full_transaction(self, |connection| {
            let accepted_origin = connection
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM layerfs_layers l
                     WHERE l.layer_stack_id = ?1 AND l.layer_id = ?2
                       AND l.result_root_id = ?3 AND l.state = 'accepted'
                       AND ((?4 = 0 AND l.creation_kind = 'genesis') OR EXISTS(
                         SELECT 1 FROM layerfs_layer_stack_transitions t
                         WHERE t.layer_stack_id = l.layer_stack_id
                           AND t.after_generation = ?4 AND t.after_layer_id = l.layer_id)))",
                    params![
                        origin.layer_stack_id.as_bytes(),
                        origin.layer_id.as_bytes(),
                        origin.root.as_bytes(),
                        i64::try_from(origin.generation)
                            .map_err(|_| EngineError::CounterOverflow)?,
                    ],
                    |row| row.get::<_, bool>(0),
                )
                .map_err(map_sqlite_error)?;
            if !accepted_origin || bundle.ancestry.origin_layer_stack_id != origin.layer_stack_id {
                return Err(EngineError::InvalidRecord("Push origin LayerStack"));
            }
            let observed = connection
                .query_row(
                    "SELECT COALESCE(sum(o.canonical_length), 0)
                     FROM layerfs_sync_object_pins p JOIN layerfs_objects o
                       ON o.object_id = p.object_id
                     WHERE p.owner_request_id = ?1 AND p.request_id = ?2
                       AND p.direction = 'push'",
                    params![transfer_id.as_bytes(), data_request_id.as_bytes()],
                    |row| row.get::<_, i64>(0),
                )
                .map_err(map_sqlite_error)?;
            if u64::try_from(observed).ok() != Some(counters.unique_bytes) {
                return Err(EngineError::InvalidRecord(
                    "Push page receiver-observed bytes",
                ));
            }
            connection
                .execute(
                    "INSERT OR IGNORE INTO layerfs_sync_receipts
                     (request_id, authority_storage_id, direction, candidate_kind,
                      candidate_id, expected_head_id, expected_generation, expected_root_id,
                      decided_head_present, decided_head_id, decided_generation, decided_root_id,
                      result, unique_bytes, resumed_bytes, retransmitted_bytes,
                      reconciliation_result)
                     VALUES (?1, ?2, 'prepare', 'layer', ?3, ?3, ?4, ?5,
                             1, ?3, ?4, ?5, 'verified_complete', 0, 0, 0,
                             'verified_complete')",
                    params![
                        verification,
                        self.storage_id(),
                        origin.layer_id.as_bytes(),
                        i64::try_from(origin.generation)
                            .map_err(|_| EngineError::CounterOverflow)?,
                        origin.root.as_bytes(),
                    ],
                )
                .map_err(map_sqlite_error)?;
            connection
                .execute(
                    "INSERT OR IGNORE INTO layerfs_version_leases
                     (lease_id, target_kind, layer_stack_id, layer_id, owner_kind,
                      owner_id, created_at)
                     VALUES (?1, 'layer', ?2, ?3, 'sync', ?4, ?5)",
                    params![
                        pin,
                        origin.layer_stack_id.as_bytes(),
                        origin.layer_id.as_bytes(),
                        transfer_id.as_bytes(),
                        unix_seconds()?,
                    ],
                )
                .map_err(map_sqlite_error)?;
            connection
                .execute(
                    "INSERT OR IGNORE INTO layerfs_branch_push_pages
                     (page_id, transfer_id, data_request_id, page_sequence, branch_id,
                      origin_authority_storage_id, origin_target_kind, origin_target_id,
                      origin_version_id, origin_generation, origin_root_id,
                      origin_verification_receipt_id, origin_pin_id, bundle, identity_version,
                      page_digest, unique_bytes, resumed_bytes, retransmitted_bytes, created_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'layer_stack', ?7, ?8, ?9, ?10,
                             ?11, ?12, ?13, 1, ?14, ?15, ?16, ?17, ?18)",
                    params![
                        page_id,
                        transfer_id.as_bytes(),
                        data_request_id.as_bytes(),
                        i64::try_from(page_sequence).map_err(|_| EngineError::CounterOverflow)?,
                        bundle.head.branch_id.as_bytes(),
                        self.storage_id(),
                        origin.layer_stack_id.as_bytes(),
                        origin.layer_id.as_bytes(),
                        i64::try_from(origin.generation)
                            .map_err(|_| EngineError::CounterOverflow)?,
                        origin.root.as_bytes(),
                        verification,
                        pin,
                        encoded,
                        page_digest,
                        i64::try_from(counters.unique_bytes)
                            .map_err(|_| EngineError::CounterOverflow)?,
                        i64::try_from(counters.resumed_bytes)
                            .map_err(|_| EngineError::CounterOverflow)?,
                        i64::try_from(counters.retransmitted_bytes)
                            .map_err(|_| EngineError::CounterOverflow)?,
                        unix_seconds()?,
                    ],
                )
                .map_err(map_sqlite_error)?;
            let exact = connection
                .query_row(
                    "SELECT transfer_id = ?2 AND data_request_id = ?3 AND page_sequence = ?4
                         AND branch_id = ?5 AND origin_authority_storage_id = ?6
                         AND origin_target_id = ?7 AND origin_version_id = ?8
                         AND origin_generation = ?9 AND origin_root_id = ?10
                         AND origin_verification_receipt_id = ?11 AND origin_pin_id = ?12
                         AND bundle = ?13 AND page_digest = ?14
                         AND unique_bytes = ?15 AND resumed_bytes = ?16
                         AND retransmitted_bytes = ?17
                     FROM layerfs_branch_push_pages WHERE page_id = ?1",
                    params![
                        page_id,
                        transfer_id.as_bytes(),
                        data_request_id.as_bytes(),
                        i64::try_from(page_sequence).map_err(|_| EngineError::CounterOverflow)?,
                        bundle.head.branch_id.as_bytes(),
                        self.storage_id(),
                        origin.layer_stack_id.as_bytes(),
                        origin.layer_id.as_bytes(),
                        i64::try_from(origin.generation)
                            .map_err(|_| EngineError::CounterOverflow)?,
                        origin.root.as_bytes(),
                        verification,
                        pin,
                        encoded,
                        page_digest,
                        i64::try_from(counters.unique_bytes)
                            .map_err(|_| EngineError::CounterOverflow)?,
                        i64::try_from(counters.resumed_bytes)
                            .map_err(|_| EngineError::CounterOverflow)?,
                        i64::try_from(counters.retransmitted_bytes)
                            .map_err(|_| EngineError::CounterOverflow)?,
                    ],
                    |row| row.get::<_, bool>(0),
                )
                .map_err(map_sqlite_error)?;
            if exact {
                Ok(())
            } else {
                Err(EngineError::InvalidRecord("Full Push page replay"))
            }
        })
    }
}

pub(crate) fn read_staged_branch_push_pages(
    connection: &rusqlite::Connection,
    transfer_id: RequestId,
    branch_id: BranchId,
) -> EngineResult<Vec<BranchPushBundle>> {
    let mut statement = connection
        .prepare(
            "SELECT page_sequence, bundle FROM layerfs_branch_push_pages
             WHERE transfer_id = ?1 ORDER BY page_sequence",
        )
        .map_err(map_sqlite_error)?;
    let mut rows = statement
        .query(params![transfer_id.as_bytes()])
        .map_err(map_sqlite_error)?;
    let mut pages = Vec::new();
    let mut records = 0_usize;
    while let Some(row) = rows.next().map_err(map_sqlite_error)? {
        if row.get::<_, i64>(0).map_err(map_sqlite_error)?
            != i64::try_from(pages.len()).map_err(|_| EngineError::CounterOverflow)?
            || pages.len() >= MAX_PUSH_OPERATION_RECORDS
        {
            return Err(EngineError::InvalidRecord("Full staged Push sequence"));
        }
        let encoded = row.get::<_, Vec<u8>>(1).map_err(map_sqlite_error)?;
        let page: BranchPushBundle = serde_json::from_slice(&encoded)
            .map_err(|_| EngineError::InvalidRecord("Full staged Push bundle"))?;
        validate_staged_push_page(&page)?;
        records = records
            .checked_add(page.operations.len())
            .and_then(|count| count.checked_add(page.child_merges.len()))
            .and_then(|count| count.checked_add(page.rollbacks.len()))
            .ok_or(EngineError::CounterOverflow)?;
        if records > MAX_PUSH_OPERATION_RECORDS || page.head.branch_id != branch_id {
            return Err(EngineError::InvalidRecord("Full staged Push history"));
        }
        pages.push(page);
    }
    Ok(pages)
}
