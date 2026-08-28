//! Accepted fetch-dependency proof and admission.

use crate::error::{map_sqlite_error, EngineError, EngineResult};
use crate::full::branch::read::{
    branch_contains_exact_version, read_branch_ancestry, read_branch_head, BranchAncestry,
};
use crate::full::branch::transition::{insert_pushed_branch_rollback, insert_pushed_child_merge};
use crate::full::closure::membership::authenticate_root;
use crate::full::layer_stack::import::import_layer_stack_snapshot;
use crate::full::layer_stack::read::read_layer_root;
use crate::full::legacy_branch::insert_branch_snapshot;
use crate::full::legacy_store::Engine;
use crate::full::operation::record::insert_pushed_operation;
use crate::full::record_id::{derive_id, object_id, BranchId, OperationId, OperationVersionId};
use crate::full::transfer::batch::{BranchPushBundle, MAX_PUSH_OPERATION_RECORDS};
use crate::working::lease::unix_seconds;
use layerfs_core::ObjectId;
use rusqlite::Connection;
use rusqlite::{params, OptionalExtension};

#[derive(Clone, Copy)]
pub(crate) struct FetchAncestryProof {
    pub(crate) operation_id: OperationId,
    pub(crate) root: ObjectId,
    pub(crate) ancestry: BranchAncestry,
}

pub(crate) fn collect_fetch_ancestry_proofs(
    bundle: &BranchPushBundle,
    proofs: &mut std::collections::BTreeMap<(BranchId, OperationVersionId), FetchAncestryProof>,
) -> EngineResult<()> {
    for operation in &bundle.operations {
        let proof = FetchAncestryProof {
            operation_id: operation.operation_id,
            root: operation.root,
            ancestry: bundle.ancestry,
        };
        if proofs
            .insert(
                (bundle.head.branch_id, operation.operation_version_id),
                proof,
            )
            .is_some()
        {
            return Err(EngineError::InvalidRecord("Fetch ancestry proof conflict"));
        }
    }
    if proofs.len() > MAX_PUSH_OPERATION_RECORDS {
        return Err(EngineError::InvalidRecord("Fetch ancestry proof page"));
    }
    for dependency in &bundle.dependencies {
        collect_fetch_ancestry_proofs(dependency, proofs)?;
    }
    Ok(())
}

pub(crate) fn import_fetch_dependency(
    engine: &Engine,
    connection: &Connection,
    bundle: &BranchPushBundle,
    source_roots: &std::collections::BTreeSet<(BranchId, ObjectId)>,
    ancestry_proofs: &std::collections::BTreeMap<
        (BranchId, OperationVersionId),
        FetchAncestryProof,
    >,
) -> EngineResult<()> {
    import_layer_stack_snapshot(engine, connection, &bundle.origin_stack, None)?;
    for dependency in &bundle.dependencies {
        import_fetch_dependency(
            engine,
            connection,
            dependency,
            source_roots,
            ancestry_proofs,
        )?;
    }
    if let Some(incumbent) = read_branch_head(connection, bundle.head.branch_id)? {
        let retained = incumbent == bundle.head
            || if bundle.head.generation == 0 && bundle.head.operation_version_id.is_none() {
                bundle.head.root == bundle.ancestry.fork_root
            } else {
                branch_contains_exact_version(connection, bundle.head)?
            };
        if retained
            && read_branch_ancestry(connection, bundle.head.branch_id)? == Some(bundle.ancestry)
        {
            return Ok(());
        }
        return Err(EngineError::InvalidRecord(
            "Fetch dependency Branch conflict",
        ));
    }
    let origin_root = read_layer_root(
        connection,
        bundle.ancestry.origin_layer_stack_id,
        bundle.ancestry.origin_layer_id,
    )?
    .ok_or(EngineError::InvalidRecord("Fetch dependency origin Layer"))?;
    match (
        bundle.ancestry.immediate_parent_branch_id,
        bundle.ancestry.fork_operation_id,
        bundle.ancestry.fork_operation_version_id,
    ) {
        (None, None, None)
            if bundle.ancestry.depth == 0 && bundle.ancestry.fork_root == origin_root => {}
        (Some(parent), Some(operation), Some(version)) if bundle.ancestry.depth > 0 => {
            let parent_ancestry = read_branch_ancestry(connection, parent)?
                .ok_or(EngineError::InvalidRecord("Fetch dependency parent Branch"))?;
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
                .map_err(map_sqlite_error)?;
            let exact_fork = match fork {
                Some(fork) => {
                    object_id(&fork.0)? == bundle.ancestry.fork_root
                        && fork.1 == "operation"
                        && fork.2.as_deref() == Some(operation.as_bytes())
                }
                None => ancestry_proofs
                    .get(&(parent, version))
                    .is_some_and(|proof| {
                        proof.operation_id == operation
                            && proof.root == bundle.ancestry.fork_root
                            && proof.ancestry == parent_ancestry
                    }),
            };
            if !exact_fork
                || parent_ancestry.origin_layer_stack_id != bundle.ancestry.origin_layer_stack_id
                || parent_ancestry.origin_layer_id != bundle.ancestry.origin_layer_id
                || parent_ancestry.depth.checked_add(1) != Some(bundle.ancestry.depth)
            {
                return Err(EngineError::InvalidRecord(
                    "Fetch dependency child ancestry",
                ));
            }
            authenticate_root(engine, connection, bundle.ancestry.fork_root)?;
        }
        _ => return Err(EngineError::InvalidRecord("Fetch dependency ancestry")),
    }
    insert_branch_snapshot(connection, bundle)?;
    let mut history = Vec::new();
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
    let mut prior_version = None;
    let mut prior_root = bundle.ancestry.fork_root;
    let mut prior_generation = 0;
    for (_, kind, index) in history {
        let next = match kind {
            0 => insert_pushed_operation(
                engine,
                connection,
                bundle,
                &bundle.operations[index],
                prior_version,
                prior_root,
                prior_generation,
            )?,
            1 => insert_pushed_child_merge(
                engine,
                connection,
                bundle.head.branch_id,
                &bundle.child_merges[index],
                prior_version,
                prior_root,
                prior_generation,
                Some(source_roots),
            )?,
            2 => insert_pushed_branch_rollback(
                connection,
                bundle.head.branch_id,
                &bundle.rollbacks[index],
                prior_version,
                prior_generation,
            )?,
            _ => unreachable!(),
        };
        prior_version = Some(next.0);
        prior_root = next.1;
        prior_generation = next.2;
    }
    if prior_version != bundle.head.operation_version_id
        || prior_root != bundle.head.root
        || prior_generation != bundle.head.generation
    {
        return Err(EngineError::InvalidRecord("Fetch dependency history head"));
    }
    let lease_id = derive_id(
        if bundle.ancestry.depth == 0 {
            b"top-level-branch-origin-lease"
        } else {
            b"child-branch-origin-lease"
        },
        &[
            bundle.head.branch_id.as_bytes(),
            bundle
                .ancestry
                .fork_operation_version_id
                .map(|id| id.0)
                .unwrap_or(bundle.ancestry.origin_layer_id.0)
                .as_slice(),
        ],
    );
    connection
        .execute(
            "INSERT INTO layerfs_version_leases
             (lease_id, target_kind, target_id, owner_kind, owner_id, created_at)
             VALUES (?1, ?2, ?3, 'branch', ?4, ?5)",
            params![
                lease_id.as_slice(),
                if bundle.ancestry.depth == 0 {
                    "layer"
                } else {
                    "operation_version"
                },
                bundle
                    .ancestry
                    .fork_operation_version_id
                    .map(|id| id.0)
                    .unwrap_or(bundle.ancestry.origin_layer_id.0)
                    .as_slice(),
                bundle.head.branch_id.as_bytes(),
                unix_seconds()?,
            ],
        )
        .map_err(map_sqlite_error)?;
    Ok(())
}
