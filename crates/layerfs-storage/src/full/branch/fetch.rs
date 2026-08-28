//! Verified accepted Branch fetch admission.

use crate::error::{map_sqlite_error, EngineError, EngineResult};
use crate::full::branch::dependency::{collect_fetch_ancestry_proofs, import_fetch_dependency};
use crate::full::branch::import::{read_fetch_branch_head, stage_fetch_branch_head};
use crate::full::branch::read::{read_branch_ancestry, read_branch_head, BranchHead};
use crate::full::branch::transition::{insert_pushed_branch_rollback, insert_pushed_child_merge};
use crate::full::closure::membership::collect_fetch_branch_roots;
use crate::full::layer_stack::import::import_layer_stack_snapshot;
use crate::full::layer_stack::read::{read_layer_root, read_layer_stack_head};
use crate::full::legacy_branch::insert_branch_snapshot;
use crate::full::legacy_store::{
    begin_product_transaction, commit_product_request, commit_product_state,
    rollback_product_transaction, Engine,
};
use crate::full::operation::record::insert_pushed_operation;
use crate::full::receipt::{
    commit_partial_fetch, commit_verified_fetch, finish_fetch_staging, insert_push_receipt,
    read_push_receipt,
};
use crate::full::record_id::{derive_id, object_id};
use crate::full::transfer::batch::{
    BranchPushBundle, BranchPushOutcome, BranchPushRequest, VerifiedFetchRequest,
    MAX_PUSH_OPERATION_RECORDS, MAX_TRANSITION_PAYLOAD_BYTES,
};
use crate::working::lease::unix_seconds;
use rusqlite::{params, OptionalExtension};

impl Engine {
    pub fn product_import_verified_branch_fetch(
        &self,
        expected: Option<BranchHead>,
        bundle: &BranchPushBundle,
        fetch: VerifiedFetchRequest,
    ) -> EngineResult<BranchPushOutcome> {
        self.product_accept_branch_bundle(expected, bundle, None, Some(fetch))
    }

    fn product_accept_branch_bundle(
        &self,
        expected: Option<BranchHead>,
        bundle: &BranchPushBundle,
        push: Option<BranchPushRequest>,
        fetch: Option<VerifiedFetchRequest>,
    ) -> EngineResult<BranchPushOutcome> {
        let history_len = bundle
            .operations
            .len()
            .checked_add(bundle.child_merges.len())
            .and_then(|count| count.checked_add(bundle.rollbacks.len()))
            .ok_or(EngineError::CounterOverflow)?;
        if history_len > MAX_PUSH_OPERATION_RECORDS {
            return Err(EngineError::InvalidRecord("Push history page required"));
        }
        if bundle.origin_stack.head.layer_stack_id != bundle.ancestry.origin_layer_stack_id
            || bundle.origin_stack.name.is_empty()
            || bundle.origin_stack.name.len() > 255
            || bundle
                .name
                .as_ref()
                .is_some_and(|name| name.is_empty() || name.len() > 255)
            || bundle
                .operations
                .iter()
                .any(|operation| operation.transition_payload.len() > MAX_TRANSITION_PAYLOAD_BYTES)
            || bundle.child_merges.iter().any(|merge| {
                merge.source_transition_payload.len() > MAX_TRANSITION_PAYLOAD_BYTES
                    || merge.applied_transition_payload.len() > MAX_TRANSITION_PAYLOAD_BYTES
            })
            || bundle.origin_stack.layers.iter().any(|layer| {
                layer.merge.as_ref().is_some_and(|merge| {
                    merge.source_transition_payload.len() > MAX_TRANSITION_PAYLOAD_BYTES
                        || merge.applied_transition_payload.len() > MAX_TRANSITION_PAYLOAD_BYTES
                })
            })
        {
            return Err(EngineError::InvalidRecord("Push history resource bound"));
        }
        if push.is_some() && bundle.base != expected {
            return Err(EngineError::InvalidRecord("Push expected head"));
        }
        if push.is_some() && !bundle.dependencies.is_empty() {
            return Err(EngineError::InvalidRecord(
                "Push dependencies require explicit publication",
            ));
        }
        if push.is_some() && (!bundle.child_merges.is_empty() || !bundle.rollbacks.is_empty()) {
            return Err(EngineError::InvalidRecord(
                "Push merge/rollback requires dedicated publication",
            ));
        }
        if push.is_some() && fetch.is_some() {
            return Err(EngineError::InvalidRecord("Branch transfer action"));
        }
        let mut connection = begin_product_transaction(self)?;
        let result = (|| {
            if let Some(request) = push {
                if let Some(outcome) = read_push_receipt(&connection, request, bundle)? {
                    return Ok(outcome);
                }
            }
            let published = read_branch_head(&connection, bundle.head.branch_id)?;
            let actual = if fetch.is_some() {
                read_fetch_branch_head(&connection, bundle.head.branch_id)?
            } else {
                published
            };
            if let (Some(fetch), false, Some(actual)) = (fetch, bundle.complete, actual) {
                stage_fetch_branch_head(&connection, fetch.durable_storage_id, published, actual)?;
            }
            if actual != expected {
                if let Some(request) = push {
                    insert_push_receipt(self, &connection, request, bundle, "conflict")?;
                    let reconciled = commit_product_request(
                        self,
                        &mut connection,
                        "layerfs_sync_receipts",
                        request.request_id,
                    )?;
                    let _ = reconciled;
                }
                return Ok(BranchPushOutcome::Conflict { actual });
            }
            let mut fetch_source_roots = None;
            let preinserted = fetch.is_some() && actual.is_none();
            if let Some(fetch) = fetch {
                import_layer_stack_snapshot(
                    self,
                    &connection,
                    &bundle.origin_stack,
                    Some((fetch.durable_storage_id, bundle.complete)),
                )?;
            }
            if push.is_some() {
                let stack =
                    read_layer_stack_head(&connection, bundle.ancestry.origin_layer_stack_id)?;
                if stack.is_none()
                    || bundle.origin_stack.base != Some(bundle.origin_stack.head)
                    || !bundle.origin_stack.complete
                    || !bundle.origin_stack.layers.is_empty()
                    || !bundle.origin_stack.transitions.is_empty()
                {
                    return Err(EngineError::InvalidRecord("Push origin LayerStack"));
                }
            }
            if fetch.is_some() {
                let mut roots = std::collections::BTreeSet::new();
                collect_fetch_branch_roots(bundle, &mut roots)?;
                fetch_source_roots = Some(roots);
                let mut proofs = std::collections::BTreeMap::new();
                collect_fetch_ancestry_proofs(bundle, &mut proofs)?;
                if preinserted {
                    insert_branch_snapshot(&connection, bundle)?;
                }
                for dependency in &bundle.dependencies {
                    import_fetch_dependency(
                        self,
                        &connection,
                        dependency,
                        fetch_source_roots.as_ref().expect("Fetch roots"),
                        &proofs,
                    )?;
                }
            }
            let origin_root = read_layer_root(
                &connection,
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
                    if bundle.ancestry.depth == 0 && bundle.ancestry.fork_root == origin_root => {}
                (Some(parent), Some(operation), Some(version)) if bundle.ancestry.depth > 0 => {
                    let parent_ancestry = read_branch_ancestry(&connection, parent)?
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
                        || parent_ancestry.origin_layer_stack_id
                            != bundle.ancestry.origin_layer_stack_id
                        || parent_ancestry.origin_layer_id != bundle.ancestry.origin_layer_id
                        || parent_ancestry.depth.checked_add(1) != Some(bundle.ancestry.depth)
                    {
                        return Err(EngineError::InvalidRecord("Push child ancestry"));
                    }
                }
                _ => return Err(EngineError::InvalidRecord("Push Branch ancestry")),
            }
            if actual.is_some()
                && read_branch_ancestry(&connection, bundle.head.branch_id)?
                    != Some(bundle.ancestry)
            {
                return Err(EngineError::InvalidRecord("Push Branch ancestry changed"));
            }
            let mut history = Vec::with_capacity(history_len);
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
            let last_head = history.last().map(|(_, kind, index)| match kind {
                0 => Some(bundle.operations[*index].operation_version_id),
                1 => Some(bundle.child_merges[*index].operation_version_id),
                2 => Some(bundle.rollbacks[*index].target_operation_version_id),
                _ => unreachable!(),
            });
            if last_head.is_some_and(|version| {
                version != bundle.head.operation_version_id
                    || history.last().map(|record| record.0) != Some(bundle.head.generation)
            }) || last_head.is_none()
                && !matches!(actual, Some(actual) if actual == bundle.head)
                && !(actual.is_none()
                    && bundle.head.generation == 0
                    && bundle.head.operation_version_id.is_none())
            {
                return Err(EngineError::InvalidRecord("Push Branch head"));
            }
            match actual {
                None => 0,
                Some(current) if current == bundle.head => {
                    if let Some(fetch) = fetch {
                        let reconciled = if bundle.complete {
                            finish_fetch_staging(&connection, current.branch_id)?;
                            commit_verified_fetch(
                                self,
                                &mut connection,
                                fetch,
                                current,
                                bundle.origin_stack.head,
                            )?
                        } else {
                            stage_fetch_branch_head(
                                &connection,
                                fetch.durable_storage_id,
                                published,
                                current,
                            )?;
                            commit_partial_fetch(self, &mut connection, fetch, current)?
                        };
                        return Ok(BranchPushOutcome::DurablyAccepted {
                            head: current,
                            reconciled,
                        });
                    }
                    if let Some(request) = push {
                        insert_push_receipt(
                            self,
                            &connection,
                            request,
                            bundle,
                            "durably_accepted",
                        )?;
                        let reconciled = commit_product_request(
                            self,
                            &mut connection,
                            "layerfs_sync_receipts",
                            request.request_id,
                        )?;
                        return Ok(BranchPushOutcome::DurablyAccepted {
                            head: current,
                            reconciled,
                        });
                    }
                    return Ok(BranchPushOutcome::DurablyAccepted {
                        head: current,
                        reconciled: true,
                    });
                }
                Some(_) => 0,
            };
            if actual.is_none() && !preinserted {
                insert_branch_snapshot(&connection, bundle)?;
            }
            let mut prior_version = actual.and_then(|head| head.operation_version_id);
            let mut prior_root = actual
                .map(|head| head.root)
                .unwrap_or(bundle.ancestry.fork_root);
            let mut prior_generation = actual.map(|head| head.generation).unwrap_or(0);
            for (_, kind, index) in &history {
                let next = match kind {
                    0 => insert_pushed_operation(
                        self,
                        &connection,
                        bundle,
                        &bundle.operations[*index],
                        prior_version,
                        prior_root,
                        prior_generation,
                    )?,
                    1 => insert_pushed_child_merge(
                        self,
                        &connection,
                        bundle.head.branch_id,
                        &bundle.child_merges[*index],
                        prior_version,
                        prior_root,
                        prior_generation,
                        fetch_source_roots.as_ref(),
                    )?,
                    2 => insert_pushed_branch_rollback(
                        &connection,
                        bundle.head.branch_id,
                        &bundle.rollbacks[*index],
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
                return Err(EngineError::InvalidRecord("Push Branch history head"));
            }
            if let Some(current) = actual {
                let changed = connection
                    .execute(
                        "UPDATE layerfs_branches
                         SET generation = ?1, head_operation_version_id = ?2
                         WHERE branch_id = ?3 AND generation = ?4
                           AND head_operation_version_id IS ?5 AND state = 'active'",
                        params![
                            i64::try_from(bundle.head.generation)
                                .map_err(|_| EngineError::CounterOverflow)?,
                            bundle
                                .head
                                .operation_version_id
                                .map(|id| id.as_bytes().as_slice().to_vec()),
                            bundle.head.branch_id.as_bytes(),
                            i64::try_from(current.generation)
                                .map_err(|_| EngineError::CounterOverflow)?,
                            current
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
            }
            let reconciled = if let Some(fetch) = fetch {
                if bundle.complete {
                    finish_fetch_staging(&connection, bundle.head.branch_id)?;
                    commit_verified_fetch(
                        self,
                        &mut connection,
                        fetch,
                        bundle.head,
                        bundle.origin_stack.head,
                    )?
                } else {
                    stage_fetch_branch_head(
                        &connection,
                        fetch.durable_storage_id,
                        published,
                        bundle.head,
                    )?;
                    commit_partial_fetch(self, &mut connection, fetch, bundle.head)?
                }
            } else if let Some(request) = push {
                insert_push_receipt(self, &connection, request, bundle, "durably_accepted")?;
                commit_product_request(
                    self,
                    &mut connection,
                    "layerfs_sync_receipts",
                    request.request_id,
                )?
            } else {
                commit_product_state(
                        self,
                        &mut connection,
                        "SELECT EXISTS(SELECT 1 FROM layerfs_branches WHERE branch_id = ?1 AND state = 'active')",
                        bundle.head.branch_id.as_bytes(),
                    )?;
                false
            };
            Ok(BranchPushOutcome::DurablyAccepted {
                head: bundle.head,
                reconciled,
            })
        })();
        rollback_product_transaction(self, &mut connection, &result);
        result
    }
}
