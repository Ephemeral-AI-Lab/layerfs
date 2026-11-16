//! Branch, project and path removals: marker writes in one write transaction.
//!
//! Split out of `history/admission.rs` to keep the product file under the
//! boundary guard's length limit; the items are the module's own.

use super::*;

/// `branch.remove`: scan the Branch scope and write the marker in one write
/// transaction.
pub(crate) fn remove_branch(
    provider: &HistoryProvider,
    branch: BranchId,
    removed_ms: i64,
    intent: &GcIntent,
) -> HistoryResult<()> {
    let postgres = postgres(provider);
    let catalog = provider.catalog_id;
    let _ = catalog;
    provider.write(|tx| {
        let owners = OwnerView::now();
        let record = branch::branch(tx, branch)?.ok_or(HistoryError::Missing(Missing::Branch))?;
        #[cfg(feature = "postgresql")]
        if postgres {
            lock_store_shared(tx, catalog)?;
            lock_scope(tx, catalog, 2, record.stack.as_slice(), true)?;
            let found = query(
                tx,
                ALL_BRANCH,
                params![
                    Some(record.stack.as_slice().to_vec()),
                    branch.as_slice().to_vec(),
                    intent.op_id.to_vec()
                ],
            )?;
            let found: Vec<GcIntent> = found
                .into_iter()
                .filter(|intent| !owners.settleable(intent, postgres))
                .collect();
            if !found.is_empty() {
                return Err(classify(&found));
            }
            lock_scope(tx, catalog, 3, branch.as_slice(), false)?;
        }
        epoch_hold(tx, postgres, record.stack)?;
        let found = settle_dead(
            tx,
            query(
                tx,
                ALL_BRANCH,
                params![
                    Some(record.stack.as_slice().to_vec()),
                    branch.as_slice().to_vec(),
                    intent.op_id.to_vec()
                ],
            )?,
            postgres,
            &owners,
        )?;
        if !found.is_empty() {
            return Err(classify(&found));
        }
        super::super::removal::mark_branch_removed(tx, branch, removed_ms)
    })
}

/// `project.delete`: the same-home retry classification, then admission and
/// the marker in one write transaction.
pub(crate) fn delete_stack(
    provider: &HistoryProvider,
    stack: LayerStackId,
    deleted_ms: i64,
    intent: &GcIntent,
) -> HistoryResult<GcDeletion> {
    let postgres = postgres(provider);
    let catalog = provider.catalog_id;
    let _ = catalog;
    provider.write(|tx| {
        let owners = OwnerView::now();
        if let Some(recorded) = layerstack::stack_deleted_ms(tx, stack)? {
            return Ok(GcDeletion::Retired {
                deleted_ms: recorded,
            });
        }
        let live = layerstack::layer_stack(tx, stack)?;
        let Some(record) = live else {
            return Ok(match layerstack::stack_deleted_ms(tx, stack)? {
                Some(recorded) => GcDeletion::Retired {
                    deleted_ms: recorded,
                },
                None => GcDeletion::Absent,
            });
        };
        #[cfg(feature = "postgresql")]
        if postgres {
            lock_store_shared(tx, catalog)?;
            let found = query(
                tx,
                ALL_PROJECT,
                params![
                    stack.as_slice().to_vec(),
                    Some(record.name.as_str().to_owned()),
                    intent.op_id.to_vec()
                ],
            )?;
            let found: Vec<GcIntent> = found
                .into_iter()
                .filter(|intent| !owners.settleable(intent, postgres))
                .collect();
            if !found.is_empty() {
                return Err(classify(&found));
            }
            lock_scope(tx, catalog, 2, stack.as_slice(), false)?;
        }
        epoch_hold(tx, postgres, stack)?;
        let found = settle_dead(
            tx,
            query(
                tx,
                ALL_PROJECT,
                params![
                    stack.as_slice().to_vec(),
                    Some(record.name.as_str().to_owned()),
                    intent.op_id.to_vec()
                ],
            )?,
            postgres,
            &owners,
        )?;
        if !found.is_empty() {
            return Err(classify(&found));
        }
        layerstack::mark_stack_deleted(tx, stack, deleted_ms)?;
        Ok(GcDeletion::Marked { deleted_ms })
    })
}

/// `project.remove_file`: admission first, then the marker and epoch bump in
/// the same write transaction.
pub(crate) fn remove_path(
    provider: &HistoryProvider,
    request: &PathRemovalRequest,
    intent: &GcIntent,
) -> HistoryResult<PathRemovalOutcome> {
    let postgres = postgres(provider);
    let catalog = provider.catalog_id;
    let _ = catalog;
    provider.write(|tx| {
        let owners = OwnerView::now();
        if layerstack::layer_stack(tx, request.stack)?.is_none() {
            return Err(HistoryError::Missing(Missing::LayerStack));
        }
        #[cfg(feature = "postgresql")]
        if postgres {
            lock_store_shared(tx, catalog)?;
            let found = query(
                tx,
                ALL_PROJECT,
                params![
                    request.stack.as_slice().to_vec(),
                    intent.project.clone(),
                    intent.op_id.to_vec()
                ],
            )?;
            let found: Vec<GcIntent> = found
                .into_iter()
                .filter(|intent| !owners.settleable(intent, postgres))
                .collect();
            if !found.is_empty() {
                return Err(classify(&found));
            }
            lock_scope(tx, catalog, 2, request.stack.as_slice(), false)?;
        }
        epoch_hold(tx, postgres, request.stack)?;
        let found = settle_dead(
            tx,
            query(
                tx,
                ALL_PROJECT,
                params![
                    request.stack.as_slice().to_vec(),
                    intent.project.clone(),
                    intent.op_id.to_vec()
                ],
            )?,
            postgres,
            &owners,
        )?;
        if !found.is_empty() {
            return Err(classify(&found));
        }
        let key = trace_key(&provider.trace_salt, request.stack, &request.path);
        let stored = one(
            tx,
            "paths_marker",
            [request.stack.as_slice(), key.as_slice()],
            |row| {
                Ok((
                    cell::<Vec<u8>>(row, 0)?,
                    unsigned(row, 1)?,
                    cell::<i64>(row, 2)?,
                ))
            },
        )?;
        let stack_high = position::stack_high_water(tx, request.stack)?;
        if let Some((stored_path, frontier, removed)) = &stored {
            if stored_path != &request.path {
                return Err(HistoryError::Integrity("path marker key"));
            }
            if stack_high <= *frontier {
                return Ok(PathRemovalOutcome {
                    kind: PathRemovalKind::UpToDate,
                    stack: request.stack,
                    path: stored_path.clone(),
                    frontier_serial: *frontier,
                    removed_ms: *removed,
                });
            }
        }
        // Spec 5: the PostgreSQL marker transaction bumps the project epoch
        // BEFORE it reads the frontier `next_history_serial - 1`. A writer
        // that read the old epoch is then either fenced by its `confirm`
        // re-check or, if it committed first, included in this frontier read.
        // SQLite's single `BEGIN IMMEDIATE` transaction already excludes a
        // writer from the read, so it bumps after the marker there.
        if postgres {
            let advanced = tx.execute("paths_advance_epoch", [request.stack.as_slice()])?;
            if advanced != 1 {
                return Err(HistoryError::Conflict("project epoch"));
            }
        }
        let frontier = position::high_water(tx)?;
        paths::mark(
            tx,
            request.stack,
            &request.path,
            &provider.trace_salt,
            frontier,
            request.removed_ms,
        )?;
        let advanced = if postgres {
            1
        } else {
            tx.execute("paths_advance_epoch", [request.stack.as_slice()])?
        };
        if advanced != 1 {
            return Err(HistoryError::Conflict("project epoch"));
        }
        Ok(PathRemovalOutcome {
            kind: PathRemovalKind::Removed,
            stack: request.stack,
            path: request.path.clone(),
            frontier_serial: frontier,
            removed_ms: request.removed_ms,
        })
    })
}

/// Every live intent recorded at one scope, for `store.status` and tests.
pub(crate) fn intents(
    provider: &HistoryProvider,
    scope_kind: GcScopeKind,
    scope_id: &[u8],
) -> HistoryResult<Vec<GcIntent>> {
    provider.read(|tx| query(tx, LIST, params![scope_kind.as_str(), scope_id.to_vec()]))
}
