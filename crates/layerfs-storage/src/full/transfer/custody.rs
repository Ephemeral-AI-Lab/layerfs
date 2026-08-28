//! Incoming transfer custody cleanup.

use crate::error::{map_sqlite_error, EngineError, EngineResult};
use crate::full::legacy_store::{commit_product_state, Engine};
use crate::full::record_id::{bytes32, RequestId};
use crate::sqlite::connection::ConnectionGuard;
use crate::working::lease::unix_seconds;
use crate::FullStorage;
use layerfs_core::ObjectId;
use rusqlite::{params, Connection, OptionalExtension};
use std::collections::HashSet;

pub(crate) fn connection_store_id(connection: &Connection) -> EngineResult<[u8; 32]> {
    connection
        .query_row(
            "SELECT store_id FROM layerfs_authority WHERE authority_id = 1",
            [],
            |row| row.get::<_, Vec<u8>>(0),
        )
        .map_err(map_sqlite_error)
        .and_then(|bytes| bytes32(&bytes, "StorageId"))
}

pub(crate) fn release_staged_push_pins(
    connection: &Connection,
    transfer_id: RequestId,
) -> EngineResult<()> {
    connection
        .execute(
            "DELETE FROM layerfs_sync_object_pins
             WHERE direction = 'push' AND owner_request_id = ?1",
            params![transfer_id.as_bytes()],
        )
        .map_err(map_sqlite_error)?;
    Ok(())
}

pub(crate) fn delete_sync_custody(
    engine: &Engine,
    connection: &mut ConnectionGuard<'_>,
    owner_request_id: RequestId,
    direction: &str,
) -> EngineResult<u64> {
    let pins = connection
        .execute(
            "DELETE FROM layerfs_sync_object_pins
             WHERE owner_request_id = ?1 AND direction = ?2",
            params![owner_request_id.as_bytes(), direction],
        )
        .map_err(map_sqlite_error)?;
    let batches = connection
        .execute(
            "DELETE FROM layerfs_sync_batch_receipts
             WHERE owner_request_id = ?1 AND direction = ?2",
            params![owner_request_id.as_bytes(), direction],
        )
        .map_err(map_sqlite_error)?;
    let pages = if direction == "push" {
        connection
            .execute(
                "DELETE FROM layerfs_branch_push_pages WHERE transfer_id = ?1",
                params![owner_request_id.as_bytes()],
            )
            .map_err(map_sqlite_error)?
    } else {
        0
    };
    let progress = connection
        .execute(
            "DELETE FROM layerfs_transfer_state
             WHERE owner_request_id = ?1 AND direction = ?2",
            params![owner_request_id.as_bytes(), direction],
        )
        .map_err(map_sqlite_error)?;
    let sql = if direction == "push" {
        "SELECT NOT EXISTS(SELECT 1 FROM layerfs_sync_object_pins
             WHERE owner_request_id = ?1 AND direction = 'push')
         AND NOT EXISTS(SELECT 1 FROM layerfs_sync_batch_receipts
             WHERE owner_request_id = ?1 AND direction = 'push')
         AND NOT EXISTS(SELECT 1 FROM layerfs_branch_push_pages WHERE transfer_id = ?1)"
    } else {
        "SELECT NOT EXISTS(SELECT 1 FROM layerfs_sync_object_pins
             WHERE owner_request_id = ?1 AND direction = 'fetch')
         AND NOT EXISTS(SELECT 1 FROM layerfs_sync_batch_receipts
             WHERE owner_request_id = ?1 AND direction = 'fetch')"
    };
    commit_product_state(engine, connection, sql, owner_request_id.as_bytes())?;
    u64::try_from(pins)
        .ok()
        .and_then(|pins| {
            u64::try_from(batches)
                .ok()
                .and_then(|batches| pins.checked_add(batches))
        })
        .and_then(|rows| {
            u64::try_from(pages)
                .ok()
                .and_then(|pages| rows.checked_add(pages))
        })
        .and_then(|rows| {
            u64::try_from(progress)
                .ok()
                .and_then(|progress| rows.checked_add(progress))
        })
        .ok_or(EngineError::CounterOverflow)
}

impl FullStorage {
    pub fn accept_canonical_batch_pinned(
        &self,
        owner_request_id: RequestId,
        request_id: RequestId,
        direction: &str,
        objects: &[(ObjectId, Vec<u8>)],
    ) -> EngineResult<()> {
        self.require_authority()?;
        if !matches!(direction, "fetch" | "push" | "prepare") || objects.is_empty() {
            return Err(EngineError::InvalidRecord("Full sync batch"));
        }
        let mut seen = HashSet::with_capacity(objects.len());
        let mut batch = blake3::Hasher::new();
        batch.update(b"layerfs.sync.batch.v1\0");
        batch.update(owner_request_id.as_bytes());
        batch.update(request_id.as_bytes());
        batch.update(direction.as_bytes());
        let mut canonical_bytes = 0_u64;
        let mut validated = Vec::with_capacity(objects.len());
        for (expected, canonical) in objects {
            if !seen.insert(*expected) {
                return Err(EngineError::InvalidRecord("duplicate sync object"));
            }
            let object =
                layerfs_core::validate_identity(canonical, *expected).map_err(|cause| {
                    EngineError::MalformedObject {
                        id: *expected,
                        cause,
                    }
                })?;
            let length =
                u64::try_from(canonical.len()).map_err(|_| EngineError::CounterOverflow)?;
            canonical_bytes = canonical_bytes
                .checked_add(length)
                .ok_or(EngineError::CounterOverflow)?;
            batch.update(expected.as_bytes());
            batch.update(&length.to_be_bytes());
            validated.push((*expected, object.kind(), canonical.as_slice()));
        }
        let batch_id = *batch.finalize().as_bytes();
        full_transaction(self, |connection| {
            for (id, kind, canonical) in &validated {
                let incumbent = connection
                    .query_row(
                        "SELECT kind, canonical_length, canonical_bytes
                         FROM layerfs_objects WHERE object_id = ?1",
                        params![id.as_bytes()],
                        |row| {
                            Ok((
                                row.get::<_, i64>(0)?,
                                row.get::<_, i64>(1)?,
                                row.get::<_, Vec<u8>>(2)?,
                            ))
                        },
                    )
                    .optional()
                    .map_err(map_sqlite_error)?;
                if let Some((stored_kind, length, bytes)) = incumbent {
                    crate::integrity::full::object::authenticate_object_row(
                        *id,
                        stored_kind,
                        length,
                        &bytes,
                    )?;
                    if stored_kind != i64::from(*kind as u8) || bytes != *canonical {
                        return Err(EngineError::ImmutableConflict("object", *id));
                    }
                } else {
                    connection
                        .execute(
                            "INSERT INTO layerfs_objects
                             (object_id, kind, canonical_length, canonical_bytes)
                             VALUES (?1, ?2, ?3, ?4)",
                            params![
                                id.as_bytes(),
                                i64::from(*kind as u8),
                                i64::try_from(canonical.len())
                                    .map_err(|_| EngineError::CounterOverflow)?,
                                canonical,
                            ],
                        )
                        .map_err(map_sqlite_error)?;
                }
                connection
                    .execute(
                        "INSERT INTO layerfs_sync_object_pins
                         (owner_request_id, request_id, direction, object_id, created_at)
                         VALUES (?1, ?2, ?3, ?4, ?5)
                         ON CONFLICT(request_id, direction, object_id) DO NOTHING",
                        params![
                            owner_request_id.as_bytes(),
                            request_id.as_bytes(),
                            direction,
                            id.as_bytes(),
                            unix_seconds()?,
                        ],
                    )
                    .map_err(map_sqlite_error)?;
            }
            let wrong_owner = connection
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM layerfs_sync_object_pins
                     WHERE request_id = ?1 AND direction = ?2 AND owner_request_id != ?3)",
                    params![
                        request_id.as_bytes(),
                        direction,
                        owner_request_id.as_bytes()
                    ],
                    |row| row.get::<_, bool>(0),
                )
                .map_err(map_sqlite_error)?;
            if wrong_owner {
                return Err(EngineError::InvalidRecord("sync pin owner"));
            }
            connection
                .execute(
                    "INSERT INTO layerfs_sync_batch_receipts
                     (batch_id, owner_request_id, request_id, direction,
                      object_count, canonical_bytes, created_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                     ON CONFLICT(batch_id) DO NOTHING",
                    params![
                        batch_id,
                        owner_request_id.as_bytes(),
                        request_id.as_bytes(),
                        direction,
                        i64::try_from(objects.len()).map_err(|_| EngineError::CounterOverflow)?,
                        i64::try_from(canonical_bytes).map_err(|_| EngineError::CounterOverflow)?,
                        unix_seconds()?,
                    ],
                )
                .map_err(map_sqlite_error)?;
            let exact = connection
                .query_row(
                    "SELECT owner_request_id = ?2, request_id = ?3, direction = ?4,
                            object_count = ?5, canonical_bytes = ?6
                     FROM layerfs_sync_batch_receipts WHERE batch_id = ?1",
                    params![
                        batch_id,
                        owner_request_id.as_bytes(),
                        request_id.as_bytes(),
                        direction,
                        i64::try_from(objects.len()).map_err(|_| EngineError::CounterOverflow)?,
                        i64::try_from(canonical_bytes).map_err(|_| EngineError::CounterOverflow)?,
                    ],
                    |row| {
                        Ok(row.get::<_, bool>(0)?
                            && row.get::<_, bool>(1)?
                            && row.get::<_, bool>(2)?
                            && row.get::<_, bool>(3)?
                            && row.get::<_, bool>(4)?)
                    },
                )
                .map_err(map_sqlite_error)?;
            if exact {
                Ok(())
            } else {
                Err(EngineError::InvalidRecord("sync batch replay"))
            }
        })
    }

    pub fn abort_sync_transfer(&self, owner: RequestId, direction: &str) -> EngineResult<u64> {
        self.require_authority()?;
        validate_direction(direction)?;
        full_transaction(self, |connection| {
            delete_full_custody(connection, owner, direction)
        })
    }

    pub fn sync_custody_rows(&self, owner: RequestId, direction: &str) -> EngineResult<u64> {
        validate_direction(direction)?;
        let connection = self.lock_connection()?;
        let rows = connection
            .query_row(
                "SELECT
                   (SELECT count(*) FROM layerfs_sync_object_pins
                    WHERE owner_request_id = ?1 AND direction = ?2)
                 + (SELECT count(*) FROM layerfs_sync_batch_receipts
                    WHERE owner_request_id = ?1 AND direction = ?2)
                 + (SELECT count(*) FROM layerfs_branch_push_pages
                    WHERE transfer_id = ?1 AND ?2 = 'push')
                 + (SELECT count(DISTINCT origin_verification_receipt_id)
                    FROM layerfs_branch_push_pages
                    WHERE transfer_id = ?1 AND ?2 = 'push')
                 + (SELECT count(*) FROM layerfs_version_leases
                    WHERE owner_kind = 'sync' AND owner_id = ?1 AND ?2 = 'push')
                 + (SELECT count(*) FROM layerfs_transfer_state
                    WHERE owner_request_id = ?1 AND direction = ?2)",
                params![owner.as_bytes(), direction],
                |row| row.get::<_, i64>(0),
            )
            .map_err(map_sqlite_error)?;
        u64::try_from(rows).map_err(|_| EngineError::CounterOverflow)
    }

    pub fn reap_one_abandoned_sync(
        &self,
        older_than_unix_seconds: i64,
    ) -> EngineResult<Option<(RequestId, String, u64)>> {
        self.require_authority()?;
        full_transaction(self, |connection| {
            let owner = connection
                .query_row(
                    "SELECT owner_request_id, direction FROM (
                         SELECT owner_request_id, direction, created_at
                           FROM layerfs_sync_object_pins
                         UNION ALL SELECT owner_request_id, direction, created_at
                           FROM layerfs_sync_batch_receipts
                         UNION ALL SELECT transfer_id, 'push', created_at
                           FROM layerfs_branch_push_pages)
                     WHERE created_at < ?1 ORDER BY created_at, owner_request_id LIMIT 1",
                    params![older_than_unix_seconds],
                    |row| Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, String>(1)?)),
                )
                .optional()
                .map_err(map_sqlite_error)?;
            let Some((owner, direction)) = owner else {
                return Ok(None);
            };
            let owner = RequestId(bytes32(&owner, "RequestId")?);
            let rows = delete_full_custody(connection, owner, &direction)?;
            Ok(Some((owner, direction, rows)))
        })
    }
}

pub(crate) fn full_transaction<T>(
    storage: &FullStorage,
    work: impl FnOnce(&Connection) -> EngineResult<T>,
) -> EngineResult<T> {
    let connection = storage.lock_connection()?;
    connection
        .execute_batch("BEGIN IMMEDIATE")
        .map_err(map_sqlite_error)?;
    let result = work(&connection);
    match result {
        Ok(value) => {
            connection
                .execute_batch("COMMIT")
                .map_err(map_sqlite_error)?;
            Ok(value)
        }
        Err(error) => {
            let _ = connection.execute_batch("ROLLBACK");
            Err(error)
        }
    }
}

fn validate_direction(direction: &str) -> EngineResult<()> {
    if matches!(direction, "fetch" | "push" | "prepare") {
        Ok(())
    } else {
        Err(EngineError::InvalidRecord("sync direction"))
    }
}

fn delete_full_custody(
    connection: &Connection,
    owner: RequestId,
    direction: &str,
) -> EngineResult<u64> {
    let verification_receipts = if direction == "push" {
        let mut statement = connection
            .prepare(
                "SELECT DISTINCT origin_verification_receipt_id
                 FROM layerfs_branch_push_pages WHERE transfer_id = ?1",
            )
            .map_err(map_sqlite_error)?;
        let receipts = statement
            .query_map(params![owner.as_bytes()], |row| row.get::<_, Vec<u8>>(0))
            .map_err(map_sqlite_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(map_sqlite_error)?;
        receipts
    } else {
        Vec::new()
    };
    let pins = connection
        .execute(
            "DELETE FROM layerfs_sync_object_pins
             WHERE owner_request_id = ?1 AND direction = ?2",
            params![owner.as_bytes(), direction],
        )
        .map_err(map_sqlite_error)?;
    let batches = connection
        .execute(
            "DELETE FROM layerfs_sync_batch_receipts
             WHERE owner_request_id = ?1 AND direction = ?2",
            params![owner.as_bytes(), direction],
        )
        .map_err(map_sqlite_error)?;
    let pages = connection
        .execute(
            "DELETE FROM layerfs_branch_push_pages
             WHERE transfer_id = ?1 AND ?2 = 'push'",
            params![owner.as_bytes(), direction],
        )
        .map_err(map_sqlite_error)?;
    let leases = connection
        .execute(
            "DELETE FROM layerfs_version_leases
             WHERE owner_kind = 'sync' AND owner_id = ?1 AND ?2 = 'push'",
            params![owner.as_bytes(), direction],
        )
        .map_err(map_sqlite_error)?;
    let mut receipts = 0_usize;
    for receipt in verification_receipts {
        receipts = receipts
            .checked_add(
                connection
                    .execute(
                        "DELETE FROM layerfs_sync_receipts
                         WHERE request_id = ?1 AND direction = 'prepare'",
                        params![receipt],
                    )
                    .map_err(map_sqlite_error)?,
            )
            .ok_or(EngineError::CounterOverflow)?;
    }
    let progress = connection
        .execute(
            "DELETE FROM layerfs_transfer_state
             WHERE owner_request_id = ?1 AND direction = ?2",
            params![owner.as_bytes(), direction],
        )
        .map_err(map_sqlite_error)?;
    [pins, batches, pages, leases, receipts, progress]
        .into_iter()
        .try_fold(0_u64, |total, rows| {
            total
                .checked_add(u64::try_from(rows).map_err(|_| EngineError::CounterOverflow)?)
                .ok_or(EngineError::CounterOverflow)
        })
}
