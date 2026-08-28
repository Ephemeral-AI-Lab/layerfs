//! Private prepared LayerStack-finalization candidates.

use crate::full::branch::read::BranchHead;
use crate::full::layer_stack::read::LayerStackHead;
use crate::full::record_id::{LayerId, LayerStackId, RequestId};
use layerfs_core::ObjectId;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct LayerCandidate {
    pub layer_stack_id: LayerStackId,
    pub layer_id: LayerId,
    pub parent_layer_id: LayerId,
    pub source: BranchHead,
    pub source_depth: u64,
    pub root: ObjectId,
    pub request_id: RequestId,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LayerCandidateRequest {
    pub source: BranchHead,
    pub expected_stack: LayerStackHead,
    pub result_root: ObjectId,
    pub source_transition: Vec<u8>,
    pub applied_transition: Vec<u8>,
    pub request_id: RequestId,
}

use crate::error::{map_sqlite_error, EngineError, EngineResult};
use crate::full::branch::read::{
    branch_contains_exact_version, read_branch_ancestry, read_branch_head,
};
use crate::full::branch::transition::insert_transition;
use crate::full::closure::membership::authenticate_root;
use crate::full::layer_stack::read::{read_layer_root, read_layer_stack_head};
use crate::full::legacy_store::{
    begin_product_transaction, commit_product_state, rollback_product_transaction, Engine,
};
use crate::full::record_id::{
    bytes32, derive_id, object_id, transition_identity, BranchId, OperationVersionId,
};
use crate::full::transfer::batch::MAX_TRANSITION_PAYLOAD_BYTES;
use crate::working::compaction::reachability::release_retained_root_if_unreferenced;
use crate::working::lease::unix_seconds;
use rusqlite::{params, OptionalExtension};

impl Engine {
    pub fn product_prepare_layer_candidate(
        &self,
        candidate: LayerCandidateRequest,
    ) -> EngineResult<LayerCandidate> {
        if candidate.source_transition.len() > MAX_TRANSITION_PAYLOAD_BYTES
            || candidate.applied_transition.len() > MAX_TRANSITION_PAYLOAD_BYTES
        {
            return Err(EngineError::InvalidRecord(
                "Merge transition resource bound",
            ));
        }
        let mut connection = begin_product_transaction(self)?;
        let result = (|| {
            if read_layer_stack_head(&connection, candidate.expected_stack.layer_stack_id)?
                != Some(candidate.expected_stack)
                || read_branch_head(&connection, candidate.source.branch_id)?
                    != Some(candidate.source)
            {
                return Err(EngineError::PublicationConflict);
            }
            let ancestry = read_branch_ancestry(&connection, candidate.source.branch_id)?
                .ok_or(EngineError::InvalidRecord("source Branch"))?;
            if ancestry.origin_layer_stack_id != candidate.expected_stack.layer_stack_id {
                return Err(EngineError::InvalidRecord("cross-tree LayerStack merge"));
            }
            let origin_root = read_layer_root(
                &connection,
                ancestry.origin_layer_stack_id,
                ancestry.origin_layer_id,
            )?
            .ok_or(EngineError::InvalidRecord("origin Layer"))?;
            let source_version = candidate
                .source
                .operation_version_id
                .ok_or(EngineError::InvalidRecord("Layer candidate source head"))?;
            authenticate_root(self, &connection, candidate.result_root)?;
            let source_delta_id = transition_identity(
                origin_root,
                candidate.source.root,
                &candidate.source_transition,
            );
            insert_transition(
                &connection,
                source_delta_id,
                origin_root,
                candidate.source.root,
                &candidate.source_transition,
            )?;
            let applied_delta_id = transition_identity(
                candidate.expected_stack.root,
                candidate.result_root,
                &candidate.applied_transition,
            );
            insert_transition(
                &connection,
                applied_delta_id,
                candidate.expected_stack.root,
                candidate.result_root,
                &candidate.applied_transition,
            )?;
            let branch_delta_id = derive_id(
                b"layer-stack-branch-delta",
                &[
                    candidate.source.branch_id.as_bytes(),
                    candidate.request_id.as_bytes(),
                    &source_delta_id,
                    &applied_delta_id,
                ],
            );
            connection
                .execute(
                    "INSERT INTO layerfs_branch_deltas
                     (branch_delta_id, purpose, source_branch_id,
                      source_branch_generation, source_branch_operation_version_id, base_root,
                      source_root, destination_root, result_root,
                      source_delta_id, applied_delta_id)
                     VALUES (?1, 'layer_stack_merge', ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                    params![
                        branch_delta_id.as_slice(),
                        candidate.source.branch_id.as_bytes(),
                        i64::try_from(candidate.source.generation)
                            .map_err(|_| EngineError::CounterOverflow)?,
                        source_version.as_bytes(),
                        origin_root.as_bytes(),
                        candidate.source.root.as_bytes(),
                        candidate.expected_stack.root.as_bytes(),
                        candidate.result_root.as_bytes(),
                        source_delta_id.as_slice(),
                        applied_delta_id.as_slice(),
                    ],
                )
                .map_err(map_sqlite_error)?;
            let layer_id = LayerId(derive_id(
                b"candidate-layer",
                &[
                    candidate.expected_stack.layer_stack_id.as_bytes(),
                    candidate.request_id.as_bytes(),
                    candidate.result_root.as_bytes(),
                ],
            ));
            connection
                .execute(
                    "INSERT INTO layerfs_layers
                     (layer_id, layer_stack_id, parent_layer_id, root_id,
                      creation_kind, source_branch_id, source_branch_depth,
                      source_branch_generation,
                      source_branch_head_operation_version_id,
                      source_branch_delta_id, state, prepared_request_id)
                     VALUES (?1, ?2, ?3, ?4, 'candidate', ?5, ?6, ?7, ?8, ?9,
                             'candidate', ?10)",
                    params![
                        layer_id.as_bytes(),
                        candidate.expected_stack.layer_stack_id.as_bytes(),
                        candidate.expected_stack.layer_id.as_bytes(),
                        candidate.result_root.as_bytes(),
                        candidate.source.branch_id.as_bytes(),
                        i64::try_from(ancestry.depth).map_err(|_| EngineError::CounterOverflow)?,
                        i64::try_from(candidate.source.generation)
                            .map_err(|_| EngineError::CounterOverflow)?,
                        source_version.as_bytes(),
                        branch_delta_id.as_slice(),
                        candidate.request_id.as_bytes(),
                    ],
                )
                .map_err(map_sqlite_error)?;
            let layer_delta_id = derive_id(
                b"layer-delta",
                &[
                    candidate.expected_stack.layer_id.as_bytes(),
                    layer_id.as_bytes(),
                    &applied_delta_id,
                ],
            );
            connection
                .execute(
                    "INSERT INTO layerfs_layer_deltas
                     (layer_delta_id, parent_layer_id, candidate_layer_id,
                      transition_delta_id, parent_root, result_root)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    params![
                        layer_delta_id.as_slice(),
                        candidate.expected_stack.layer_id.as_bytes(),
                        layer_id.as_bytes(),
                        applied_delta_id.as_slice(),
                        candidate.expected_stack.root.as_bytes(),
                        candidate.result_root.as_bytes(),
                    ],
                )
                .map_err(map_sqlite_error)?;
            let source_lease_id = derive_id(
                b"layer-candidate-source-lease",
                &[source_version.as_bytes(), candidate.request_id.as_bytes()],
            );
            connection
                .execute(
                    "INSERT INTO layerfs_version_leases
                     (lease_id, target_kind, target_id, owner_kind, owner_id, created_at)
                     VALUES (?1, 'operation_version', ?2, 'layer_candidate', ?3, ?4)",
                    params![
                        source_lease_id.as_slice(),
                        source_version.as_bytes(),
                        candidate.request_id.as_bytes(),
                        unix_seconds()?,
                    ],
                )
                .map_err(map_sqlite_error)?;
            let lease_id = derive_id(
                b"layer-candidate-lease",
                &[layer_id.as_bytes(), candidate.request_id.as_bytes()],
            );
            connection
                .execute(
                    "INSERT INTO layerfs_version_leases
                     (lease_id, target_kind, target_id, owner_kind, owner_id, created_at)
                     VALUES (?1, 'layer', ?2, 'layer_candidate', ?3, ?4)",
                    params![
                        lease_id.as_slice(),
                        layer_id.as_bytes(),
                        candidate.request_id.as_bytes(),
                        unix_seconds()?,
                    ],
                )
                .map_err(map_sqlite_error)?;
            connection
                .execute(
                    "INSERT INTO layerfs_retained_roots (root_id) VALUES (?1)
                     ON CONFLICT(root_id) DO NOTHING",
                    params![candidate.result_root.as_bytes()],
                )
                .map_err(map_sqlite_error)?;
            commit_product_state(
                self,
                &mut connection,
                "SELECT EXISTS(SELECT 1 FROM layerfs_layers WHERE layer_id = ?1 AND state = 'candidate')",
                layer_id.as_bytes(),
            )?;
            Ok(LayerCandidate {
                layer_stack_id: candidate.expected_stack.layer_stack_id,
                layer_id,
                parent_layer_id: candidate.expected_stack.layer_id,
                source: candidate.source,
                source_depth: ancestry.depth,
                root: candidate.result_root,
                request_id: candidate.request_id,
            })
        })();
        rollback_product_transaction(self, &mut connection, &result);
        result
    }

    pub fn product_layer_candidates_after(
        &self,
        after: Option<LayerId>,
        limit: usize,
    ) -> EngineResult<Vec<LayerCandidate>> {
        if limit == 0 || limit > 1024 {
            return Err(EngineError::InvalidRecord("Layer candidate page limit"));
        }
        let connection = self.lock_connection()?;
        let mut statement = connection
            .prepare(
                "SELECT l.layer_stack_id, l.layer_id, l.parent_layer_id,
                        l.source_branch_id, l.source_branch_generation,
                        l.source_branch_head_operation_version_id,
                        l.source_branch_depth, v.root_id, l.root_id,
                        l.prepared_request_id
                 FROM layerfs_layers l
                 JOIN layerfs_operation_versions v
                   ON v.branch_id = l.source_branch_id
                  AND v.operation_version_id = l.source_branch_head_operation_version_id
                 WHERE l.state = 'candidate' AND (?1 IS NULL OR l.layer_id > ?1)
                 ORDER BY l.layer_id LIMIT ?2",
            )
            .map_err(map_sqlite_error)?;
        let candidates = statement
            .query_map(
                params![
                    after.map(|id| id.as_bytes().as_slice().to_vec()),
                    i64::try_from(limit).map_err(|_| EngineError::CounterOverflow)?,
                ],
                |row| {
                    Ok((
                        row.get::<_, Vec<u8>>(0)?,
                        row.get::<_, Vec<u8>>(1)?,
                        row.get::<_, Vec<u8>>(2)?,
                        row.get::<_, Vec<u8>>(3)?,
                        row.get::<_, i64>(4)?,
                        row.get::<_, Vec<u8>>(5)?,
                        row.get::<_, i64>(6)?,
                        row.get::<_, Vec<u8>>(7)?,
                        row.get::<_, Vec<u8>>(8)?,
                        row.get::<_, Vec<u8>>(9)?,
                    ))
                },
            )
            .map_err(map_sqlite_error)?
            .map(|row| {
                let row = row.map_err(map_sqlite_error)?;
                Ok(LayerCandidate {
                    layer_stack_id: LayerStackId(bytes32(&row.0, "LayerStackId")?),
                    layer_id: LayerId(bytes32(&row.1, "LayerId")?),
                    parent_layer_id: LayerId(bytes32(&row.2, "LayerId")?),
                    source: BranchHead {
                        branch_id: BranchId(bytes32(&row.3, "BranchId")?),
                        generation: u64::try_from(row.4)
                            .map_err(|_| EngineError::InvalidRecord("Branch generation"))?,
                        operation_version_id: Some(OperationVersionId(bytes32(
                            &row.5,
                            "OperationVersionId",
                        )?)),
                        root: object_id(&row.7)?,
                    },
                    source_depth: u64::try_from(row.6)
                        .map_err(|_| EngineError::InvalidRecord("Branch depth"))?,
                    root: object_id(&row.8)?,
                    request_id: RequestId(bytes32(&row.9, "RequestId")?),
                })
            })
            .collect();
        candidates
    }

    pub fn product_drop_layer_candidate(&self, layer_id: LayerId) -> EngineResult<bool> {
        let mut connection = begin_product_transaction(self)?;
        let result = (|| {
            let candidate = connection
                .query_row(
                    "SELECT root_id, prepared_request_id, state FROM layerfs_layers
                     WHERE layer_id = ?1 AND creation_kind = 'candidate'",
                    params![layer_id.as_bytes()],
                    |row| {
                        Ok((
                            row.get::<_, Vec<u8>>(0)?,
                            row.get::<_, Vec<u8>>(1)?,
                            row.get::<_, String>(2)?,
                        ))
                    },
                )
                .optional()
                .map_err(map_sqlite_error)?
                .ok_or(EngineError::InvalidRecord("Layer candidate"))?;
            if candidate.2 == "dropped" {
                commit_product_state(
                    self,
                    &mut connection,
                    "SELECT EXISTS(SELECT 1 FROM layerfs_layers
                     WHERE layer_id = ?1 AND state = 'dropped')",
                    layer_id.as_bytes(),
                )?;
                return Ok(true);
            }
            if candidate.2 != "candidate" {
                return Err(EngineError::InvalidRecord("accepted Layer candidate"));
            }
            let changed = connection
                .execute(
                    "UPDATE layerfs_layers SET state = 'dropped'
                     WHERE layer_id = ?1 AND state = 'candidate'",
                    params![layer_id.as_bytes()],
                )
                .map_err(map_sqlite_error)?;
            if changed != 1 {
                return Err(EngineError::PublicationConflict);
            }
            connection
                .execute(
                    "DELETE FROM layerfs_version_leases
                     WHERE owner_kind = 'layer_candidate' AND owner_id = ?1",
                    params![candidate.1],
                )
                .map_err(map_sqlite_error)?;
            release_retained_root_if_unreferenced(&connection, &candidate.0)?;
            commit_product_state(
                self,
                &mut connection,
                "SELECT EXISTS(SELECT 1 FROM layerfs_layers
                 WHERE layer_id = ?1 AND state = 'dropped')",
                layer_id.as_bytes(),
            )?;
            Ok(false)
        })();
        rollback_product_transaction(self, &mut connection, &result);
        result
    }

    pub fn product_import_layer_candidate(
        &self,
        candidate: LayerCandidate,
        expected_stack: LayerStackHead,
    ) -> EngineResult<LayerCandidate> {
        let connection = self.lock_connection()?;
        let incumbent = connection
            .query_row(
                "SELECT layer_stack_id, parent_layer_id, root_id,
                        source_branch_id, source_branch_depth,
                        source_branch_generation,
                        source_branch_head_operation_version_id,
                        prepared_request_id, state
                 FROM layerfs_layers WHERE layer_id = ?1",
                params![candidate.layer_id.as_bytes()],
                |row| {
                    Ok((
                        row.get::<_, Vec<u8>>(0)?,
                        row.get::<_, Vec<u8>>(1)?,
                        row.get::<_, Vec<u8>>(2)?,
                        row.get::<_, Vec<u8>>(3)?,
                        row.get::<_, i64>(4)?,
                        row.get::<_, i64>(5)?,
                        row.get::<_, Vec<u8>>(6)?,
                        row.get::<_, Vec<u8>>(7)?,
                        row.get::<_, String>(8)?,
                    ))
                },
            )
            .optional()
            .map_err(map_sqlite_error)?;
        if let Some(incumbent) = incumbent {
            if incumbent.0.as_slice() != candidate.layer_stack_id.as_bytes()
                || incumbent.1.as_slice() != candidate.parent_layer_id.as_bytes()
                || incumbent.2.as_slice() != candidate.root.as_bytes()
                || incumbent.3.as_slice() != candidate.source.branch_id.as_bytes()
                || u64::try_from(incumbent.4).ok() != Some(candidate.source_depth)
                || u64::try_from(incumbent.5).ok() != Some(candidate.source.generation)
                || incumbent.6.as_slice()
                    != candidate
                        .source
                        .operation_version_id
                        .ok_or(EngineError::InvalidRecord("Layer candidate source"))?
                        .as_bytes()
                || incumbent.7.as_slice() != candidate.request_id.as_bytes()
                || !matches!(incumbent.8.as_str(), "candidate" | "accepted")
                || !branch_contains_exact_version(&connection, candidate.source)?
            {
                return Err(EngineError::InvalidRecord(
                    "Layer candidate identity conflict",
                ));
            }
            return Ok(candidate);
        }
        drop(connection);
        let prepared = self.product_prepare_layer_candidate(LayerCandidateRequest {
            source: candidate.source,
            expected_stack,
            result_root: candidate.root,
            source_transition: Vec::new(),
            applied_transition: Vec::new(),
            request_id: candidate.request_id,
        })?;
        if prepared != candidate {
            return Err(EngineError::InvalidRecord("Layer candidate identity"));
        }
        Ok(prepared)
    }
}
