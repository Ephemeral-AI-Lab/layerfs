//! Branch, project and path removals: marker writes in one write transaction.
//!
//! Split out of `history/admission.rs` to keep the product file under the
//! boundary guard's length limit; the items are the module's own.

use super::intents::*;
use layerfs_history::Liveness;

/// The admission every removal runs before its marker, inside the marker's
/// own write transaction. On PostgreSQL it takes the Store key shared, the
/// project key shared too when `hold_project` (a Branch removal), scans once
/// for a live conflicting intent and then takes the scope's own key
/// `exclusive` (class, id). On both backends it then holds the project epoch
/// and scans again, settling the intents dead owners left. `scan` is the
/// statement and its parameters; a conflict is refused as [`classify`] names
/// it. The eight parameters are one transaction's own state, the two keys it
/// takes and the one scan it runs; bundling them would hide that they always
/// travel together and are never defaulted.
#[allow(clippy::too_many_arguments)]
fn admit(
    tx: &Transaction<'_>,
    provider: &HistoryProvider,
    owners: &OwnerView,
    intent: &GcIntent,
    stack: LayerStackId,
    hold_project: bool,
    exclusive: (u8, &[u8]),
    scan: (&str, Vec<Param>),
) -> HistoryResult<()> {
    let postgres = postgres(provider);
    let (statement, scope) = scan;
    let mut place = PlaceView::for_intent(intent);
    if postgres {
        let catalog = provider.catalog_id;
        lock_store_shared(tx, catalog)?;
        if hold_project {
            lock_scope(tx, catalog, 2, stack.as_slice(), true)?;
        }
        let mut live = Vec::new();
        for intent in query(tx, statement, scope.clone())? {
            if place.settleable(tx, &intent, postgres, owners)? {
                continue;
            }
            let mut intent = intent;
            if place.liveness(tx, &intent, postgres, owners)? == Liveness::Uncertain {
                intent.uncertain = true;
            }
            live.push(intent);
        }
        if !live.is_empty() {
            return Err(classify(&live));
        }
        lock_scope(tx, catalog, exclusive.0, exclusive.1, false)?;
    }
    epoch_hold(tx, postgres, stack)?;
    let found = settle_dead(
        tx,
        query(tx, statement, scope)?,
        postgres,
        owners,
        &mut place,
    )?;
    if !found.is_empty() {
        return Err(classify(&found));
    }
    Ok(())
}

/// `branch.remove`: scan the Branch scope and write the marker in one write
/// transaction.
pub(crate) fn remove_branch(
    provider: &HistoryProvider,
    branch: BranchId,
    removed_ms: i64,
    intent: &GcIntent,
) -> HistoryResult<()> {
    provider.write(|tx| {
        let owners = OwnerView::now();
        let record = branch::branch(tx, branch)?.ok_or(HistoryError::Missing(Missing::Branch))?;
        admit(
            tx,
            provider,
            &owners,
            intent,
            record.stack,
            true,
            (3, branch.as_slice()),
            (
                ALL_BRANCH,
                params![
                    Some(record.stack.as_slice().to_vec()),
                    branch.as_slice().to_vec(),
                    intent.op_id.to_vec()
                ],
            ),
        )?;
        crate::history::graph::removal::mark_branch_removed(tx, branch, removed_ms)
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
        admit(
            tx,
            provider,
            &owners,
            intent,
            stack,
            false,
            (2, stack.as_slice()),
            (
                ALL_PROJECT,
                params![
                    stack.as_slice().to_vec(),
                    Some(record.name.as_str().to_owned()),
                    intent.op_id.to_vec()
                ],
            ),
        )?;
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
    provider.write(|tx| {
        let owners = OwnerView::now();
        if layerstack::layer_stack(tx, request.stack)?.is_none() {
            return Err(HistoryError::Missing(Missing::LayerStack));
        }
        admit(
            tx,
            provider,
            &owners,
            intent,
            request.stack,
            false,
            (2, request.stack.as_slice()),
            (
                ALL_PROJECT,
                params![
                    request.stack.as_slice().to_vec(),
                    intent.project.clone(),
                    intent.op_id.to_vec()
                ],
            ),
        )?;
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
