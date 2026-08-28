//! Selected accepted-closure tracking reads.

use crate::error::{map_sqlite_error, EngineError, EngineResult};
use crate::full::branch::read::BranchHead;
use crate::full::legacy_store::Engine;
use crate::full::record_id::{derive_id, OperationVersionId};
use crate::full::transfer::batch::VerifiedFetchRequest;
use layerfs_core::ObjectId;
use rusqlite::Connection;
use rusqlite::{params, OptionalExtension};

impl Engine {
    pub fn product_has_verified_branch_tracking(
        &self,
        durable_storage_id: [u8; 32],
        head: BranchHead,
    ) -> EngineResult<bool> {
        let connection = self.lock_connection()?;
        connection
            .query_row(
                "SELECT EXISTS(
                     SELECT 1 FROM layerfs_durable_tracking_refs
                     WHERE durable_storage_id = ?1 AND target_kind = 'branch'
                       AND target_id = ?2 AND generation = ?3 AND root_id = ?4
                       AND target_version_id IS ?5
                       AND status = 'verified_complete')",
                params![
                    durable_storage_id.as_slice(),
                    head.branch_id.as_bytes(),
                    i64::try_from(head.generation).map_err(|_| EngineError::CounterOverflow)?,
                    head.root.as_bytes(),
                    head.operation_version_id
                        .map(|id| id.as_bytes().as_slice().to_vec()),
                ],
                |row| row.get::<_, bool>(0),
            )
            .map_err(map_sqlite_error)
    }
}

pub(crate) fn insert_verified_tracking_ref(
    connection: &Connection,
    request: VerifiedFetchRequest,
    target_kind: &str,
    target_id: &[u8; 32],
    target_version_id: Option<OperationVersionId>,
    generation: u64,
    root: ObjectId,
) -> EngineResult<()> {
    if !matches!(target_kind, "branch" | "layer") {
        return Err(EngineError::InvalidRecord("DurableTrackingRef kind"));
    }
    let version_bytes = target_version_id.map_or([0; 32], |id| id.0);
    let tracking_ref_id = derive_id(
        b"durable-tracking-ref",
        &[
            request.durable_storage_id.as_slice(),
            target_kind.as_bytes(),
            target_id,
            &version_bytes,
            &generation.to_be_bytes(),
            root.as_bytes(),
        ],
    );
    let tracked = connection
        .query_row(
            "SELECT tracking_ref_id, target_version_id, root_id,
                    verification_receipt_id, status
             FROM layerfs_durable_tracking_refs
             WHERE durable_storage_id = ?1 AND target_kind = ?2
               AND target_id = ?3 AND generation = ?4",
            params![
                request.durable_storage_id.as_slice(),
                target_kind,
                target_id,
                i64::try_from(generation).map_err(|_| EngineError::CounterOverflow)?,
            ],
            |row| {
                Ok((
                    row.get::<_, Vec<u8>>(0)?,
                    row.get::<_, Option<Vec<u8>>>(1)?,
                    row.get::<_, Vec<u8>>(2)?,
                    row.get::<_, Vec<u8>>(3)?,
                    row.get::<_, String>(4)?,
                ))
            },
        )
        .optional()
        .map_err(map_sqlite_error)?;
    match tracked {
        Some((id, stored_version, stored_root, _receipt, status))
            if id.as_slice() == tracking_ref_id
                && stored_version
                    == target_version_id.map(|id| id.as_bytes().as_slice().to_vec())
                && stored_root.as_slice() == root.as_bytes()
                && status == "verified_complete" => {}
        Some(_) => return Err(EngineError::InvalidRecord("DurableTrackingRef conflict")),
        None => {
            connection
                .execute(
                    "INSERT INTO layerfs_durable_tracking_refs
                     (tracking_ref_id, durable_storage_id, target_kind, target_id,
                      target_version_id, generation, root_id, verification_receipt_id, status)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8,
                             'verified_complete')",
                    params![
                        tracking_ref_id.as_slice(),
                        request.durable_storage_id.as_slice(),
                        target_kind,
                        target_id,
                        target_version_id.map(|id| id.as_bytes().as_slice().to_vec()),
                        i64::try_from(generation).map_err(|_| EngineError::CounterOverflow)?,
                        root.as_bytes(),
                        request.request_id.as_bytes(),
                    ],
                )
                .map_err(map_sqlite_error)?;
        }
    }
    Ok(())
}
