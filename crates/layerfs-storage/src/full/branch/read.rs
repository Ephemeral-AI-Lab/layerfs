//! Branch heads, ancestry, and accepted-history reads.

use crate::full::record_id::{
    BranchId, LayerId, LayerStackId, OperationId, OperationVersionId, RequestId,
};
use layerfs_core::ObjectId;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BranchHead {
    pub branch_id: BranchId,
    pub generation: u64,
    pub operation_version_id: Option<OperationVersionId>,
    pub root: ObjectId,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BranchAncestry {
    pub immediate_parent_branch_id: Option<BranchId>,
    pub fork_operation_id: Option<OperationId>,
    pub fork_operation_version_id: Option<OperationVersionId>,
    pub fork_root: ObjectId,
    pub origin_layer_stack_id: LayerStackId,
    pub origin_layer_id: LayerId,
    pub depth: u64,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum VersionRef {
    Layer {
        layer_stack_id: LayerStackId,
        layer_id: LayerId,
        root: ObjectId,
    },
    OperationVersion {
        branch_id: BranchId,
        operation_version_id: OperationVersionId,
        root: ObjectId,
    },
}

impl VersionRef {
    pub const fn root(self) -> ObjectId {
        match self {
            Self::Layer { root, .. } | Self::OperationVersion { root, .. } => root,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum BranchRollbackOutcome {
    WorkingRecorded { head: BranchHead, reconciled: bool },
    Conflict { actual: BranchHead },
    Blocked,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BranchRollbackPublication {
    pub expected: BranchHead,
    pub target: OperationVersionId,
    pub request_id: RequestId,
    pub accepted: BranchHead,
}

use crate::error::{map_sqlite_error, EngineError, EngineResult};
use crate::full::branch::import::{read_fetch_branch_head, read_published_fetch_branch_head};
use crate::full::legacy_store::Engine;
use crate::full::record_id::{bytes32, object_id};
use crate::full::store::FullStorage;
use crate::working::binding::effective_branch_base;
use rusqlite::Connection;
use rusqlite::{params, OptionalExtension};

impl Engine {
    pub fn product_branch_head(&self, branch_id: BranchId) -> EngineResult<Option<BranchHead>> {
        let connection = self.lock_connection()?;
        read_branch_head(&connection, branch_id)
    }

    pub fn product_branch_has_special_history_after(
        &self,
        branch_id: BranchId,
        generation: u64,
    ) -> EngineResult<bool> {
        let connection = self.lock_connection()?;
        connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM layerfs_branch_transitions
                 WHERE branch_id = ?1 AND after_generation > ?2
                   AND action_kind != 'operation_commit')",
                params![
                    branch_id.as_bytes(),
                    i64::try_from(generation).map_err(|_| EngineError::CounterOverflow)?,
                ],
                |row| row.get::<_, bool>(0),
            )
            .map_err(map_sqlite_error)
    }

    pub fn product_fetch_resume_branch_head(
        &self,
        branch_id: BranchId,
    ) -> EngineResult<Option<BranchHead>> {
        let connection = self.lock_connection()?;
        read_fetch_branch_head(&connection, branch_id)
    }

    pub fn product_contains_branch_head(&self, head: BranchHead) -> EngineResult<bool> {
        let connection = self.lock_connection()?;
        if head.generation == 0 && head.operation_version_id.is_none() {
            return Ok(read_branch_ancestry(&connection, head.branch_id)?
                .is_some_and(|ancestry| ancestry.fork_root == head.root));
        }
        branch_contains_exact_version(&connection, head)
    }

    pub fn product_branch_contains_root(
        &self,
        branch_id: BranchId,
        root: ObjectId,
    ) -> EngineResult<bool> {
        let connection = self.lock_connection()?;
        let fork = read_branch_ancestry(&connection, branch_id)?
            .is_some_and(|ancestry| ancestry.fork_root == root);
        if fork {
            return Ok(true);
        }
        connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM layerfs_operation_versions
                 WHERE branch_id = ?1 AND root_id = ?2
                   AND NOT EXISTS(
                       SELECT 1 FROM layerfs_released_versions r
                       WHERE r.target_kind = 'operation_version'
                         AND r.owner_id = layerfs_operation_versions.branch_id
                         AND r.version_id = layerfs_operation_versions.operation_version_id))",
                params![branch_id.as_bytes(), root.as_bytes()],
                |row| row.get::<_, bool>(0),
            )
            .map_err(map_sqlite_error)
    }

    pub fn product_branch_ancestry(
        &self,
        branch_id: BranchId,
    ) -> EngineResult<Option<BranchAncestry>> {
        let connection = self.lock_connection()?;
        read_branch_ancestry(&connection, branch_id)
    }

    pub fn product_pin_branch_version(&self, head: BranchHead) -> EngineResult<VersionRef> {
        let connection = self.lock_connection()?;
        let retained = if head.generation == 0 && head.operation_version_id.is_none() {
            read_branch_ancestry(&connection, head.branch_id)?
                .is_some_and(|ancestry| ancestry.fork_root == head.root)
        } else {
            branch_contains_exact_version(&connection, head)?
        };
        if !retained {
            return Err(EngineError::InvalidRecord("Branch version"));
        }
        effective_branch_base(&connection, head)
    }

    pub fn product_validate_version_ref(&self, version: VersionRef) -> EngineResult<()> {
        let connection = self.lock_connection()?;
        let present = match version {
            VersionRef::Layer {
                layer_stack_id,
                layer_id,
                root,
            } => connection
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM layerfs_layers
                     WHERE layer_stack_id = ?1 AND layer_id = ?2 AND root_id = ?3
                       AND NOT EXISTS(
                           SELECT 1 FROM layerfs_released_versions r
                           WHERE r.target_kind = 'layer'
                             AND r.owner_id = layerfs_layers.layer_stack_id
                             AND r.version_id = layerfs_layers.layer_id))",
                    params![
                        layer_stack_id.as_bytes(),
                        layer_id.as_bytes(),
                        root.as_bytes()
                    ],
                    |row| row.get::<_, bool>(0),
                )
                .map_err(map_sqlite_error)?,
            VersionRef::OperationVersion {
                branch_id,
                operation_version_id,
                root,
            } => connection
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM layerfs_operation_versions
                     WHERE branch_id = ?1 AND operation_version_id = ?2 AND root_id = ?3
                       AND NOT EXISTS(
                           SELECT 1 FROM layerfs_released_versions r
                           WHERE r.target_kind = 'operation_version'
                             AND r.owner_id = layerfs_operation_versions.branch_id
                             AND r.version_id = layerfs_operation_versions.operation_version_id))",
                    params![
                        branch_id.as_bytes(),
                        operation_version_id.as_bytes(),
                        root.as_bytes()
                    ],
                    |row| row.get::<_, bool>(0),
                )
                .map_err(map_sqlite_error)?,
        };
        if !present {
            return Err(EngineError::InvalidRecord("VersionRef"));
        }
        Ok(())
    }
}

impl FullStorage {
    pub fn authoritative_branch_head(
        &self,
        branch_id: BranchId,
    ) -> EngineResult<Option<BranchHead>> {
        self.require_authority()?;
        let connection = self.lock_connection()?;
        read_full_branch_head(&connection, branch_id)
    }

    pub fn authoritative_branch_ancestry(
        &self,
        branch_id: BranchId,
    ) -> EngineResult<Option<BranchAncestry>> {
        self.require_authority()?;
        let connection = self.lock_connection()?;
        read_branch_ancestry(&connection, branch_id)
    }
}

pub(crate) fn read_full_branch_head(
    connection: &Connection,
    id: BranchId,
) -> EngineResult<Option<BranchHead>> {
    connection
        .query_row(
            "SELECT b.generation, b.head_operation_version_id,
                    COALESCE(v.result_root_id, b.fork_root_id)
             FROM layerfs_branches b
             LEFT JOIN layerfs_operation_versions v
               ON v.branch_id = b.branch_id
              AND v.operation_version_id = b.head_operation_version_id
             WHERE b.branch_id = ?1 AND b.state = 'active'",
            params![id.as_bytes()],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, Option<Vec<u8>>>(1)?,
                    row.get::<_, Vec<u8>>(2)?,
                ))
            },
        )
        .optional()
        .map_err(map_sqlite_error)?
        .map(|(generation, version, root)| {
            Ok(BranchHead {
                branch_id: id,
                generation: u64::try_from(generation)
                    .map_err(|_| EngineError::InvalidRecord("Full Branch generation"))?,
                operation_version_id: version
                    .as_deref()
                    .map(|id| bytes32(id, "OperationVersionId").map(OperationVersionId))
                    .transpose()?,
                root: object_id(&root)?,
            })
        })
        .transpose()
}
pub(crate) fn read_branch_head(
    connection: &Connection,
    id: BranchId,
) -> EngineResult<Option<BranchHead>> {
    if let Some(published) = read_published_fetch_branch_head(connection, id)? {
        return Ok(published);
    }
    read_database_branch_head(connection, id)
}

pub(crate) fn read_database_branch_head(
    connection: &Connection,
    id: BranchId,
) -> EngineResult<Option<BranchHead>> {
    connection
        .query_row(
            "SELECT b.generation, b.head_operation_version_id,
                    COALESCE(v.root_id, b.fork_root_id)
             FROM layerfs_branches b
             LEFT JOIN layerfs_operation_versions v
               ON v.branch_id = b.branch_id
              AND v.operation_version_id = b.head_operation_version_id
             WHERE b.branch_id = ?1 AND b.state = 'active'",
            params![id.as_bytes()],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, Option<Vec<u8>>>(1)?,
                    row.get::<_, Vec<u8>>(2)?,
                ))
            },
        )
        .optional()
        .map_err(map_sqlite_error)?
        .map(|(generation, version, root)| {
            Ok(BranchHead {
                branch_id: id,
                generation: u64::try_from(generation)
                    .map_err(|_| EngineError::InvalidRecord("Branch generation"))?,
                operation_version_id: version
                    .map(|bytes| -> EngineResult<OperationVersionId> {
                        Ok(OperationVersionId(bytes32(&bytes, "OperationVersionId")?))
                    })
                    .transpose()?,
                root: object_id(&root)?,
            })
        })
        .transpose()
}

pub(crate) fn branch_head_at_generation(
    connection: &Connection,
    branch_id: BranchId,
    generation: u64,
) -> EngineResult<BranchHead> {
    if generation == 0 {
        let ancestry = read_branch_ancestry(connection, branch_id)?
            .ok_or(EngineError::InvalidRecord("Branch ancestry"))?;
        return Ok(BranchHead {
            branch_id,
            generation,
            operation_version_id: None,
            root: ancestry.fork_root,
        });
    }
    connection
        .query_row(
            "SELECT bt.after_operation_version_id, v.root_id
             FROM layerfs_branch_transitions bt
             JOIN layerfs_operation_versions v
               ON v.branch_id = bt.branch_id
              AND v.operation_version_id = bt.after_operation_version_id
             WHERE bt.branch_id = ?1 AND bt.after_generation = ?2",
            params![
                branch_id.as_bytes(),
                i64::try_from(generation).map_err(|_| EngineError::CounterOverflow)?
            ],
            |row| Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, Vec<u8>>(1)?)),
        )
        .optional()
        .map_err(map_sqlite_error)?
        .map(|(version, root)| -> EngineResult<BranchHead> {
            Ok(BranchHead {
                branch_id,
                generation,
                operation_version_id: Some(OperationVersionId(bytes32(
                    &version,
                    "OperationVersionId",
                )?)),
                root: object_id(&root)?,
            })
        })
        .transpose()?
        .ok_or(EngineError::InvalidRecord("Branch generation"))
}

pub(crate) fn read_branch_ancestry(
    connection: &Connection,
    id: BranchId,
) -> EngineResult<Option<BranchAncestry>> {
    connection
        .query_row(
            "SELECT immediate_parent_branch_id, fork_operation_id,
                    fork_operation_version_id, fork_root_id,
                    origin_layer_stack_id, origin_layer_id, depth
             FROM layerfs_branches WHERE branch_id = ?1",
            params![id.as_bytes()],
            |row| {
                Ok((
                    row.get::<_, Option<Vec<u8>>>(0)?,
                    row.get::<_, Option<Vec<u8>>>(1)?,
                    row.get::<_, Option<Vec<u8>>>(2)?,
                    row.get::<_, Vec<u8>>(3)?,
                    row.get::<_, Vec<u8>>(4)?,
                    row.get::<_, Vec<u8>>(5)?,
                    row.get::<_, i64>(6)?,
                ))
            },
        )
        .optional()
        .map_err(map_sqlite_error)?
        .map(|(parent, operation, version, root, stack, layer, depth)| {
            Ok(BranchAncestry {
                immediate_parent_branch_id: parent
                    .map(|bytes| -> EngineResult<BranchId> {
                        Ok(BranchId(bytes32(&bytes, "BranchId")?))
                    })
                    .transpose()?,
                fork_operation_id: operation
                    .map(|bytes| -> EngineResult<OperationId> {
                        Ok(OperationId(bytes32(&bytes, "OperationId")?))
                    })
                    .transpose()?,
                fork_operation_version_id: version
                    .map(|bytes| -> EngineResult<OperationVersionId> {
                        Ok(OperationVersionId(bytes32(&bytes, "OperationVersionId")?))
                    })
                    .transpose()?,
                fork_root: object_id(&root)?,
                origin_layer_stack_id: LayerStackId(bytes32(&stack, "LayerStackId")?),
                origin_layer_id: LayerId(bytes32(&layer, "LayerId")?),
                depth: u64::try_from(depth)
                    .map_err(|_| EngineError::InvalidRecord("Branch depth"))?,
            })
        })
        .transpose()
}

pub(crate) fn branch_contains_exact_version(
    connection: &Connection,
    head: BranchHead,
) -> EngineResult<bool> {
    let Some(version) = head.operation_version_id else {
        return Ok(false);
    };
    connection
        .query_row(
            "SELECT EXISTS(
                 SELECT 1 FROM layerfs_operation_versions v
                 JOIN layerfs_branch_transitions t
                   ON t.branch_id = v.branch_id
                  AND t.after_operation_version_id = v.operation_version_id
                 WHERE v.branch_id = ?1 AND v.operation_version_id = ?2
                   AND v.root_id = ?3 AND t.after_generation = ?4
                   AND NOT EXISTS(
                       SELECT 1 FROM layerfs_released_versions r
                       WHERE r.target_kind = 'operation_version'
                         AND r.owner_id = v.branch_id
                         AND r.version_id = v.operation_version_id))",
            params![
                head.branch_id.as_bytes(),
                version.as_bytes(),
                head.root.as_bytes(),
                i64::try_from(head.generation).map_err(|_| EngineError::CounterOverflow)?,
            ],
            |row| row.get::<_, bool>(0),
        )
        .map_err(map_sqlite_error)
}

pub(crate) fn branch_contains_exact_historical_version(
    connection: &Connection,
    head: BranchHead,
) -> EngineResult<bool> {
    let Some(version) = head.operation_version_id else {
        return Ok(false);
    };
    connection
        .query_row(
            "SELECT EXISTS(
                 SELECT 1 FROM layerfs_operation_versions v
                 JOIN layerfs_branch_transitions t
                   ON t.branch_id = v.branch_id
                  AND t.after_operation_version_id = v.operation_version_id
                 WHERE v.branch_id = ?1 AND v.operation_version_id = ?2
                   AND v.root_id = ?3 AND t.after_generation = ?4)",
            params![
                head.branch_id.as_bytes(),
                version.as_bytes(),
                head.root.as_bytes(),
                i64::try_from(head.generation).map_err(|_| EngineError::CounterOverflow)?,
            ],
            |row| row.get::<_, bool>(0),
        )
        .map_err(map_sqlite_error)
}
