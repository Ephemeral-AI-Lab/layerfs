use super::super::contract::EvalResult;
use super::super::error::display_error;
use super::super::row::contract::EngineDelta;
use super::super::row::output::{exclusive_leaf_ns, timer_ns};
use super::contract::AttributionArm;
use super::observation::AttributionObservation;
use crate::legacy_full::{
    OperationDiagnostics, ProjectionFacts, ProjectionSyncFacts, ProjectionTimer,
    ProjectionWriteFacts,
};

pub(in crate::stage1_materialize) fn scratch_observation_json(
    operation: &OperationDiagnostics,
) -> String {
    format!(
        concat!(
            "{{\"tables\":{},\"statements\":{},\"rows\":{},\"high_water_bytes\":{},",
            "\"owner_setup_statements\":{},\"derived_setup_statements\":{},",
            "\"operation_statements\":{},\"store_reopens\":{},",
            "\"store_inspection_statements\":{},\"store_inspection_wall_ns\":{},",
            "\"setup_wall_ns\":{},\"operation_wall_ns\":{}}}"
        ),
        operation.scratch_tables,
        operation.scratch_statements,
        operation.scratch_rows,
        operation.scratch_high_water_bytes,
        operation.scratch_owner_setup_statements,
        operation.scratch_derived_setup_statements,
        operation.scratch_operation_statements,
        operation.scratch_store_reopens,
        operation.scratch_store_inspection_statements,
        operation.scratch_store_inspection_wall_ns,
        operation.scratch_setup_wall_ns,
        operation.scratch_operation_wall_ns,
    )
}

pub(in crate::stage1_materialize) fn engine_sql(engine: &EngineDelta) -> EvalResult<u64> {
    engine
        .publication_statements
        .checked_add(engine.live_verified_integrity_statements)
        .and_then(|value| value.checked_add(engine.primary_read_statements))
        .and_then(|value| value.checked_add(engine.reconciliation_statements))
        .and_then(|value| value.checked_add(engine.compaction_statements))
        .ok_or_else(|| "Engine SQL equation overflow".to_owned())
}

pub(in crate::stage1_materialize) fn scratch_sql(
    operation: &OperationDiagnostics,
) -> EvalResult<u64> {
    operation
        .scratch_owner_setup_statements
        .checked_add(operation.scratch_derived_setup_statements)
        .and_then(|value| value.checked_add(operation.scratch_operation_statements))
        .ok_or_else(|| "scratch SQL equation overflow".to_owned())
}

pub(in crate::stage1_materialize) fn fact_count_exact(
    attempts: u64,
    successes: u64,
    failures: u64,
) -> bool {
    successes.checked_add(failures) == Some(attempts)
}

pub(in crate::stage1_materialize) fn fact_sum_exact(
    expected: u64,
    mut values: impl Iterator<Item = u64>,
) -> bool {
    values.try_fold(0_u64, u64::checked_add) == Some(expected)
}

pub(in crate::stage1_materialize) fn fact_timer_sum_exact(
    aggregate: ProjectionTimer,
    mut owners: impl Iterator<Item = ProjectionTimer>,
) -> bool {
    owners.try_fold(0_u64, |total, owner| {
        (owner.availability == aggregate.availability)
            .then(|| total.checked_add(owner.nanoseconds))
            .flatten()
    }) == Some(aggregate.nanoseconds)
}

pub(in crate::stage1_materialize) fn sync_fact_exact(fact: ProjectionSyncFacts) -> bool {
    fact_count_exact(fact.attempts, fact.successes, fact.failures)
        && [
            fact.requested.process_crash_reconciled,
            fact.requested.host_crash_ordered,
            fact.requested.device_flush_requested,
            fact.requested.power_loss_qualified,
        ]
        .into_iter()
        .try_fold(0_u64, u64::checked_add)
            == Some(fact.attempts)
        && [
            fact.achieved.process_crash_reconciled,
            fact.achieved.host_crash_ordered,
            fact.achieved.device_flush_requested,
            fact.achieved.power_loss_qualified,
        ]
        .into_iter()
        .try_fold(0_u64, u64::checked_add)
            == Some(fact.successes)
}

pub(in crate::stage1_materialize) fn sync_aggregate_exact(
    aggregate: ProjectionSyncFacts,
    owners: &[ProjectionSyncFacts],
) -> bool {
    sync_fact_exact(aggregate)
        && owners.iter().copied().all(sync_fact_exact)
        && fact_sum_exact(
            aggregate.attempts,
            owners.iter().map(|owner| owner.attempts),
        )
        && fact_sum_exact(
            aggregate.successes,
            owners.iter().map(|owner| owner.successes),
        )
        && fact_sum_exact(
            aggregate.failures,
            owners.iter().map(|owner| owner.failures),
        )
        && fact_sum_exact(
            aggregate.requested.process_crash_reconciled,
            owners
                .iter()
                .map(|owner| owner.requested.process_crash_reconciled),
        )
        && fact_sum_exact(
            aggregate.requested.host_crash_ordered,
            owners
                .iter()
                .map(|owner| owner.requested.host_crash_ordered),
        )
        && fact_sum_exact(
            aggregate.requested.device_flush_requested,
            owners
                .iter()
                .map(|owner| owner.requested.device_flush_requested),
        )
        && fact_sum_exact(
            aggregate.requested.power_loss_qualified,
            owners
                .iter()
                .map(|owner| owner.requested.power_loss_qualified),
        )
        && fact_sum_exact(
            aggregate.achieved.process_crash_reconciled,
            owners
                .iter()
                .map(|owner| owner.achieved.process_crash_reconciled),
        )
        && fact_sum_exact(
            aggregate.achieved.host_crash_ordered,
            owners.iter().map(|owner| owner.achieved.host_crash_ordered),
        )
        && fact_sum_exact(
            aggregate.achieved.device_flush_requested,
            owners
                .iter()
                .map(|owner| owner.achieved.device_flush_requested),
        )
        && fact_sum_exact(
            aggregate.achieved.power_loss_qualified,
            owners
                .iter()
                .map(|owner| owner.achieved.power_loss_qualified),
        )
        && fact_timer_sum_exact(aggregate.wall, owners.iter().map(|owner| owner.wall))
}

pub(in crate::stage1_materialize) fn write_aggregate_exact(
    aggregate: ProjectionWriteFacts,
    owners: &[ProjectionWriteFacts],
) -> bool {
    fact_count_exact(aggregate.attempts, aggregate.successes, aggregate.failures)
        && owners
            .iter()
            .all(|owner| fact_count_exact(owner.attempts, owner.successes, owner.failures))
        && fact_sum_exact(
            aggregate.attempts,
            owners.iter().map(|owner| owner.attempts),
        )
        && fact_sum_exact(
            aggregate.successes,
            owners.iter().map(|owner| owner.successes),
        )
        && fact_sum_exact(
            aggregate.failures,
            owners.iter().map(|owner| owner.failures),
        )
        && fact_sum_exact(aggregate.bytes, owners.iter().map(|owner| owner.bytes))
        && fact_timer_sum_exact(aggregate.wall, owners.iter().map(|owner| owner.wall))
}

pub(in crate::stage1_materialize) fn projection_facts_exact(facts: ProjectionFacts) -> bool {
    [
        facts.workspace_setup,
        facts.workspace_root_create_open,
        facts.staging_create_open,
        facts.recovery_marker_create,
        facts.name_preflight,
        facts.temp_create,
        facts.content_flush,
        facts.metadata_validate,
        facts.metadata_apply,
        facts.metadata_preinstall_verify,
        facts.metadata_postinstall_verify,
        facts.root_binding_revalidate,
        facts.authority_completion,
    ]
    .iter()
    .all(|fact| fact_count_exact(fact.attempts, fact.successes, fact.failures))
        && write_aggregate_exact(
            facts.aggregate_native_write,
            &[
                facts.workspace_marker_write,
                facts.content_write,
                facts.metadata_value_write,
            ],
        )
        && sync_aggregate_exact(
            facts.regular_file_sync,
            &[
                facts.recovery_marker_file_sync,
                facts.content_temp_file_sync,
                facts.post_hardlink_file_sync,
            ],
        )
        && sync_aggregate_exact(
            facts.directory_sync,
            &[
                facts.staging_directory_sync,
                facts.root_parent_directory_sync,
                facts.install_parent_directory_sync,
                facts.dirty_tree_directory_sync,
                facts.final_root_directory_sync,
            ],
        )
        && fact_count_exact(
            facts.replace.attempts,
            facts.replace.successes,
            facts.replace.failures,
        )
        && fact_count_exact(
            facts.cleanup.attempts,
            facts.cleanup.successes,
            facts.cleanup.failures,
        )
        && facts.cleanup.residue == facts.cleanup.failures
}

pub(in crate::stage1_materialize) fn successful_projection_facts_exact(
    facts: ProjectionFacts,
) -> bool {
    projection_facts_exact(facts) && facts.cleanup.failures == 0 && facts.cleanup.residue == 0
}

pub(in crate::stage1_materialize) fn attribution_timer_equation(
    arm: AttributionArm,
    observation: &AttributionObservation,
) -> EvalResult<(u64, u64, i128)> {
    if arm == AttributionArm::Complete {
        let (leaf_ns, dispatch_ns) = exclusive_leaf_ns(&observation.row)?;
        let residual = i128::try_from(observation.row.product_wall_ns).map_err(display_error)?
            - i128::from(leaf_ns);
        return Ok((leaf_ns, dispatch_ns, residual));
    }
    let wall = u64::try_from(observation.row.product_wall_ns).map_err(display_error)?;
    let named = if matches!(arm, AttributionArm::Null | AttributionArm::Digest) {
        let engine = &observation.row.engine;
        [
            engine.connection_mutex_wait_ns,
            engine.trust_guard_ns,
            engine.nonpayload_query_ns,
            engine.payload_query_ns,
            engine.identity_authentication_ns,
            engine.role_decode_ns,
            engine.counter_merge_ns,
            observation.row.operation.scratch_store_inspection_wall_ns,
            observation.row.operation.scratch_setup_wall_ns,
            observation.row.operation.scratch_operation_wall_ns,
        ]
        .into_iter()
        .try_fold(0_u64, |total, value| total.checked_add(value))
        .ok_or_else(|| "source attribution timer overflow".to_owned())?
    } else {
        projection_leaf_ns(observation.row.operation.projection)?
    };
    let dispatch = wall
        .checked_sub(named)
        .ok_or_else(|| "named attribution timers exceed operation wall".to_owned())?;
    Ok((wall, dispatch, 0))
}

pub(in crate::stage1_materialize) fn projection_leaf_ns(
    projection: ProjectionFacts,
) -> EvalResult<u64> {
    [
        timer_ns(projection.workspace_root_create_open.wall)?,
        timer_ns(projection.staging_create_open.wall)?,
        timer_ns(projection.recovery_marker_create.wall)?,
        timer_ns(projection.name_preflight.wall)?,
        timer_ns(projection.temp_create.wall)?,
        timer_ns(projection.workspace_marker_write.wall)?,
        timer_ns(projection.content_write.wall)?,
        timer_ns(projection.metadata_value_write.wall)?,
        timer_ns(projection.content_flush.wall)?,
        timer_ns(projection.metadata_validate.wall)?,
        timer_ns(projection.metadata_apply.wall)?,
        timer_ns(projection.metadata_preinstall_verify.wall)?,
        timer_ns(projection.metadata_postinstall_verify.wall)?,
        timer_ns(projection.root_binding_revalidate.wall)?,
        timer_ns(projection.recovery_marker_file_sync.wall)?,
        timer_ns(projection.content_temp_file_sync.wall)?,
        timer_ns(projection.post_hardlink_file_sync.wall)?,
        timer_ns(projection.staging_directory_sync.wall)?,
        timer_ns(projection.root_parent_directory_sync.wall)?,
        timer_ns(projection.install_parent_directory_sync.wall)?,
        timer_ns(projection.dirty_tree_directory_sync.wall)?,
        timer_ns(projection.final_root_directory_sync.wall)?,
        timer_ns(projection.replace.wall)?,
        timer_ns(projection.cleanup.wall)?,
    ]
    .into_iter()
    .try_fold(0_u64, |total, value| total.checked_add(value))
    .ok_or_else(|| "projection attribution timer overflow".to_owned())
}
