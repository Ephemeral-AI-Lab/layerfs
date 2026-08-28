//! Private top-level and child Branch creation.

use crate::error::{map_sqlite_error, EngineError, EngineResult};
use crate::full::branch::read::{read_branch_ancestry, BranchHead};
use crate::full::layer_stack::read::{read_layer_stack_head, LayerStackHead};
use crate::full::legacy_store::{
    begin_product_transaction, commit_product_state, rollback_product_transaction, Engine,
};
use crate::full::record_id::{derive_id, object_id, BranchId};
use crate::working::lease::unix_seconds;
use crate::working::operation::record::OperationRecordRef;
use rusqlite::{params, OptionalExtension};

impl Engine {
    pub fn product_create_top_level_branch(
        &self,
        branch_id: BranchId,
        name: Option<&str>,
        origin: LayerStackHead,
    ) -> EngineResult<BranchHead> {
        if name.is_some_and(|name| name.is_empty() || name.len() > 255) {
            return Err(EngineError::InvalidRecord("Branch name"));
        }
        let mut connection = begin_product_transaction(self)?;
        let result = (|| {
            if read_layer_stack_head(&connection, origin.layer_stack_id)? != Some(origin) {
                return Err(EngineError::PublicationConflict);
            }
            connection
                .execute(
                    "INSERT INTO layerfs_branches
                     (branch_id, name, fork_root_id, origin_layer_stack_id, origin_layer_id,
                      depth, generation, state)
                     VALUES (?1, ?2, ?3, ?4, ?5, 0, 0, 'active')",
                    params![
                        branch_id.as_bytes(),
                        name,
                        origin.root.as_bytes(),
                        origin.layer_stack_id.as_bytes(),
                        origin.layer_id.as_bytes(),
                    ],
                )
                .map_err(map_sqlite_error)?;
            let lease_id = derive_id(
                b"top-level-branch-origin-lease",
                &[branch_id.as_bytes(), origin.layer_id.as_bytes()],
            );
            connection
                .execute(
                    "INSERT INTO layerfs_version_leases
                     (lease_id, target_kind, target_id, owner_kind, owner_id, created_at)
                     VALUES (?1, 'layer', ?2, 'branch', ?3, ?4)",
                    params![
                        lease_id.as_slice(),
                        origin.layer_id.as_bytes(),
                        branch_id.as_bytes(),
                        unix_seconds()?,
                    ],
                )
                .map_err(map_sqlite_error)?;
            commit_product_state(
                self,
                &mut connection,
                "SELECT EXISTS(SELECT 1 FROM layerfs_branches WHERE branch_id = ?1 AND state = 'active')",
                branch_id.as_bytes(),
            )?;
            Ok(BranchHead {
                branch_id,
                generation: 0,
                operation_version_id: None,
                root: origin.root,
            })
        })();
        rollback_product_transaction(self, &mut connection, &result);
        result
    }

    pub fn product_create_child_branch(
        &self,
        branch_id: BranchId,
        name: Option<&str>,
        origin: OperationRecordRef,
    ) -> EngineResult<BranchHead> {
        if name.is_some_and(|name| name.is_empty() || name.len() > 255) {
            return Err(EngineError::InvalidRecord("Branch name"));
        }
        let mut connection = begin_product_transaction(self)?;
        let result = (|| {
            let (root, created_by_kind, operation) = connection
                .query_row(
                    "SELECT root_id, created_by_kind, created_by_operation_id
                     FROM layerfs_operation_versions
                     WHERE branch_id = ?1 AND operation_version_id = ?2",
                    params![
                        origin.parent_branch_id.as_bytes(),
                        origin.operation_version_id.as_bytes()
                    ],
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
                .ok_or(EngineError::InvalidRecord("child Branch origin"))?;
            if created_by_kind != "operation"
                || operation.as_deref() != Some(origin.operation_id.as_bytes())
                || object_id(&root)? != origin.root
            {
                return Err(EngineError::InvalidRecord("child Branch origin"));
            }
            let parent = read_branch_ancestry(&connection, origin.parent_branch_id)?
                .ok_or(EngineError::InvalidRecord("parent Branch"))?;
            let depth = parent
                .depth
                .checked_add(1)
                .ok_or(EngineError::CounterOverflow)?;
            connection
                .execute(
                    "INSERT INTO layerfs_branches
                     (branch_id, name, immediate_parent_branch_id, fork_operation_id,
                      fork_operation_version_id, fork_root_id, origin_layer_stack_id,
                      origin_layer_id, depth, generation, state)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 0, 'active')",
                    params![
                        branch_id.as_bytes(),
                        name,
                        origin.parent_branch_id.as_bytes(),
                        origin.operation_id.as_bytes(),
                        origin.operation_version_id.as_bytes(),
                        origin.root.as_bytes(),
                        parent.origin_layer_stack_id.as_bytes(),
                        parent.origin_layer_id.as_bytes(),
                        i64::try_from(depth).map_err(|_| EngineError::CounterOverflow)?,
                    ],
                )
                .map_err(map_sqlite_error)?;
            let lease_id = derive_id(
                b"child-branch-origin-lease",
                &[branch_id.as_bytes(), origin.operation_version_id.as_bytes()],
            );
            connection
                .execute(
                    "INSERT INTO layerfs_version_leases
                     (lease_id, target_kind, target_id, owner_kind, owner_id, created_at)
                     VALUES (?1, 'operation_version', ?2, 'branch', ?3, ?4)",
                    params![
                        lease_id.as_slice(),
                        origin.operation_version_id.as_bytes(),
                        branch_id.as_bytes(),
                        unix_seconds()?,
                    ],
                )
                .map_err(map_sqlite_error)?;
            commit_product_state(
                self,
                &mut connection,
                "SELECT EXISTS(SELECT 1 FROM layerfs_branches WHERE branch_id = ?1 AND state = 'active')",
                branch_id.as_bytes(),
            )?;
            Ok(BranchHead {
                branch_id,
                generation: 0,
                operation_version_id: None,
                root: origin.root,
            })
        })();
        rollback_product_transaction(self, &mut connection, &result);
        result
    }
}
