//! Accepted Branch history export and closure paging.

use crate::error::{map_sqlite_error, EngineError, EngineResult};
use crate::full::branch::read::{read_branch_ancestry, BranchHead};
use crate::full::layer_stack::read::{read_layer_stack_head, LayerStackHead};
use crate::full::legacy_store::{
    begin_product_transaction, checked_add, commit_product_state, rollback_product_transaction,
    Engine,
};
use crate::full::record_id::{bytes32, derive_id, BranchId, RequestId};
use crate::full::transfer::batch::{BranchPushBundle, PushedRelease};
use crate::integrity::authenticated_closure_for_each;
use crate::working::lease::unix_seconds;
use layerfs_core::ObjectId;
use rusqlite::params;

impl Engine {
    pub fn product_export_branch_push(
        &self,
        branch_id: BranchId,
        base: Option<BranchHead>,
    ) -> EngineResult<BranchPushBundle> {
        let connection = self.lock_connection()?;
        let ancestry = read_branch_ancestry(&connection, branch_id)?
            .ok_or(EngineError::InvalidRecord("Branch ancestry"))?;
        let stack = read_layer_stack_head(&connection, ancestry.origin_layer_stack_id)?
            .ok_or(EngineError::InvalidRecord("LayerStack"))?;
        drop(connection);
        self.product_export_branch_push_one(branch_id, base, Some(stack))
    }

    pub fn product_export_branch_fetch(
        &self,
        branch_id: BranchId,
    ) -> EngineResult<BranchPushBundle> {
        self.product_export_branch_fetch_page(branch_id, None, None)
    }

    pub fn product_export_branch_fetch_page(
        &self,
        branch_id: BranchId,
        base: Option<BranchHead>,
        origin_stack_base: Option<LayerStackHead>,
    ) -> EngineResult<BranchPushBundle> {
        self.product_export_branch_push_one(branch_id, base, origin_stack_base)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn product_branch_fetch_object_page(
        &self,
        branch_id: BranchId,
        base: Option<BranchHead>,
        origin_stack_base: Option<LayerStackHead>,
        expected_head: BranchHead,
        expected_stack_head: LayerStackHead,
        after: Option<ObjectId>,
        limit: usize,
    ) -> EngineResult<Vec<ObjectId>> {
        let bundle = self.product_export_branch_fetch_page(branch_id, base, origin_stack_base)?;
        if bundle.head != expected_head || bundle.origin_stack.head != expected_stack_head {
            return Err(EngineError::PublicationConflict);
        }
        let mut roots = std::collections::BTreeSet::new();
        collect_fetch_roots(&bundle, &mut roots)?;
        if limit == 0 || limit > 1024 {
            return Err(EngineError::InvalidRecord("closure page limit"));
        }
        let base_generation = base.map_or(0, |head| head.generation).to_be_bytes();
        let base_version = base
            .and_then(|head| head.operation_version_id)
            .map_or([0; 32], |id| id.0);
        let base_root = base.map_or([0; 32], |head| head.root.to_bytes());
        let stack_generation = origin_stack_base
            .map_or(0, |head| head.generation)
            .to_be_bytes();
        let stack_layer = origin_stack_base.map_or([0; 32], |head| head.layer_id.0);
        let exported_generation = bundle.head.generation.to_be_bytes();
        let exported_version = bundle.head.operation_version_id.map_or([0; 32], |id| id.0);
        let exported_root = bundle.head.root.to_bytes();
        let exported_stack_generation = bundle.origin_stack.head.generation.to_be_bytes();
        let exported_stack_layer = bundle.origin_stack.head.layer_id.0;
        let exported_stack_root = bundle.origin_stack.head.root.to_bytes();
        let closure_id = derive_id(
            b"fetch-closure",
            &[
                branch_id.as_bytes(),
                &base_generation,
                &base_version,
                &base_root,
                &stack_generation,
                &stack_layer,
                &exported_generation,
                &exported_version,
                &exported_root,
                &exported_stack_generation,
                &exported_stack_layer,
                &exported_stack_root,
            ],
        );
        let mut connection = begin_product_transaction(self)?;
        let result = (|| {
            let present = connection
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM layerfs_fetch_closure_items
                     WHERE closure_id = ?1)",
                    params![closure_id.as_slice()],
                    |row| row.get::<_, bool>(0),
                )
                .map_err(map_sqlite_error)?;
            if !present {
                self.bump(|counters| checked_add(&mut counters.fetch_closure_builds, 1))?;
                let created_at = unix_seconds()?;
                authenticated_closure_for_each(
                    &connection,
                    &self.path,
                    self.store_id,
                    roots,
                    |id| {
                        connection
                            .execute(
                                "INSERT INTO layerfs_fetch_closure_items
                                 (closure_id, object_id, created_at) VALUES (?1, ?2, ?3)",
                                params![closure_id.as_slice(), id.as_bytes(), created_at],
                            )
                            .map_err(map_sqlite_error)?;
                        Ok(())
                    },
                )?;
            }
            let page = connection
                .prepare(
                    "SELECT object_id FROM layerfs_fetch_closure_items
                     WHERE closure_id = ?1 AND (?2 IS NULL OR object_id > ?2)
                     ORDER BY object_id LIMIT ?3",
                )
                .map_err(map_sqlite_error)?
                .query_map(
                    params![
                        closure_id.as_slice(),
                        after.map(|id| id.as_bytes().as_slice().to_vec()),
                        i64::try_from(limit).map_err(|_| EngineError::CounterOverflow)?,
                    ],
                    |row| row.get::<_, Vec<u8>>(0),
                )
                .map_err(map_sqlite_error)?
                .map(|row| {
                    ObjectId::from_bytes(&row.map_err(map_sqlite_error)?).map_err(Into::into)
                })
                .collect::<EngineResult<Vec<_>>>()?;
            self.bump(|counters| checked_add(&mut counters.fetch_closure_pages, 1))?;
            if page.is_empty() {
                connection
                    .execute(
                        "DELETE FROM layerfs_fetch_closure_items WHERE closure_id = ?1",
                        params![closure_id.as_slice()],
                    )
                    .map_err(map_sqlite_error)?;
                commit_product_state(
                    self,
                    &mut connection,
                    "SELECT NOT EXISTS(SELECT 1 FROM layerfs_fetch_closure_items
                     WHERE closure_id = ?1)",
                    &closure_id,
                )?;
            } else {
                commit_product_state(
                    self,
                    &mut connection,
                    "SELECT EXISTS(SELECT 1 FROM layerfs_fetch_closure_items
                     WHERE closure_id = ?1)",
                    &closure_id,
                )?;
            }
            Ok(page)
        })();
        rollback_product_transaction(self, &mut connection, &result);
        result
    }
}

pub(crate) fn collect_fetch_roots(
    bundle: &BranchPushBundle,
    roots: &mut std::collections::BTreeSet<ObjectId>,
) -> EngineResult<()> {
    roots.insert(bundle.ancestry.fork_root);
    let head_released =
        bundle.head.operation_version_id.is_some_and(|head| {
            bundle.operations.iter().any(|operation| {
                operation.operation_version_id == head && operation.release.is_some()
            }) || bundle
                .child_merges
                .iter()
                .any(|merge| merge.operation_version_id == head && merge.release.is_some())
        });
    if !head_released {
        roots.insert(bundle.head.root);
    }
    for layer in &bundle.origin_stack.layers {
        if layer.release.is_none() {
            roots.insert(layer.root);
        }
    }
    for operation in &bundle.operations {
        if operation.release.is_none() {
            roots.insert(operation.root);
        }
    }
    for merge in &bundle.child_merges {
        if merge.release.is_none() {
            roots.insert(merge.root);
        }
    }
    for dependency in &bundle.dependencies {
        collect_fetch_roots(dependency, roots)?;
    }
    if roots.len() > 4096 {
        return Err(EngineError::InvalidRecord("Fetch root closure bound"));
    }
    Ok(())
}

pub(crate) fn pushed_release(
    generation: Option<i64>,
    request_id: Option<Vec<u8>>,
) -> EngineResult<Option<PushedRelease>> {
    match (generation, request_id) {
        (None, None) => Ok(None),
        (Some(generation), Some(request_id)) => Ok(Some(PushedRelease {
            generation: u64::try_from(generation)
                .map_err(|_| EngineError::InvalidRecord("release generation"))?,
            request_id: RequestId(bytes32(&request_id, "RequestId")?),
        })),
        _ => Err(EngineError::InvalidRecord("release record")),
    }
}
