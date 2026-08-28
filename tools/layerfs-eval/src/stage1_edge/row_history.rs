use super::artifact::display_error;
use super::campaign::Campaign;
use super::context::set_failure_phase;
use super::engine_counters::{verify_phase_partition, EngineDelta, PhaseCounterDelta};
use super::locality::content_counters;
use super::operation_json::counters_json;
use super::oracle::compare_canonical_range;
use super::receipt_model::{HistoryProbeReceipt, OracleReceipt, Phase, RowReceipt};
use super::resources::{
    observe_external_resources, observe_row_resources, row_residual, unavailable_defaults,
};
use super::row_physical::{history_custody_json, history_root_indices};
use super::schedule_model::PieceTable;
use crate::legacy_full::{Diagnostics, LayerFs, RefState};
use crate::stage1_fixture::EvalResult;
use std::path::Path;
use std::time::Instant;
#[allow(clippy::too_many_arguments)]
pub(crate) fn run_history_row(
    campaign: &mut Campaign<'_>,
    retained: &LayerFs,
    store: &Path,
    roots: &[RefState],
    snapshots: &[PieceTable],
    backing: &[u8],
    session: u8,
    work: &Path,
) -> EvalResult<()> {
    let id = if session <= 3 {
        format!("C04-{session:03}")
    } else {
        format!("C06-{:03}", session - 3)
    };
    let schedule = campaign.scheduled(&id)?;
    let row_started = Instant::now();
    set_failure_phase("verified_open");
    let verified_started = Instant::now();
    let opened = LayerFs::open(store).map_err(display_error)?;
    let verified_wall = verified_started.elapsed().as_nanos();
    let after_open = opened.fs.counter_snapshot().map_err(display_error)?;
    let head_index = usize::from(session) * 5;
    if opened.ref_state != roots[head_index] {
        return Err(format!("history session {session} recovered exact head"));
    }
    let before = opened.fs.diagnostics().map_err(display_error)?;
    set_failure_phase("history_read");
    let history_started = Instant::now();
    let mut operation = crate::legacy_full::OperationDiagnostics::default();
    let mut history_probes = Vec::new();
    for &root_index in history_root_indices(session)? {
        let table = snapshots
            .get(root_index)
            .ok_or_else(|| format!("missing oracle snapshot R{root_index}"))?;
        let probe_length = 65_536_u64;
        let middle = table.logical_length / 2 - probe_length / 2;
        let end = table
            .logical_length
            .checked_sub(probe_length)
            .ok_or_else(|| "history end probe underflow".to_owned())?;
        for (ordinal, start) in [0, middle, end].into_iter().enumerate() {
            let probe_before = opened.fs.counter_snapshot().map_err(display_error)?;
            let probe_started = Instant::now();
            let counters = compare_canonical_range(
                &opened.fs,
                roots[root_index].root,
                table,
                backing,
                start,
                probe_length,
            )?;
            let probe_wall_ns = probe_started.elapsed().as_nanos();
            let probe_after = opened.fs.counter_snapshot().map_err(display_error)?;
            let probe_engine = EngineDelta::between(&probe_before, &probe_after)?;
            probe_engine.verify_read_only()?;
            let content = content_counters(&counters)?;
            if counters.rope.payload_bytes_read != probe_length
                || counters.native != Default::default()
                || content.cdc_bytes_scanned != 0
                || content.payload_bytes_written != 0
                || (ordinal == 0
                    && (counters.namespace.nodes_read == 0 || counters.inode_table.nodes_read == 0))
                || (ordinal != 0
                    && (counters.namespace.nodes_read != 0 || counters.inode_table.nodes_read != 0))
            {
                return Err(format!(
                    "history R{root_index} probe {} plan/payload/read-only equation",
                    ordinal + 1
                ));
            }
            history_probes.push(HistoryProbeReceipt {
                root_index,
                ordinal: u8::try_from(ordinal + 1).map_err(display_error)?,
                start,
                length: probe_length,
                wall_ns: probe_wall_ns,
                engine: probe_engine,
                operation: counters,
            });
            operation = operation.merge(counters).map_err(display_error)?;
        }
    }
    let digest = campaign
        .root_digests
        .get(head_index)
        .cloned()
        .ok_or_else(|| format!("missing retained full-byte digest R{head_index}"))?;
    let history_wall = history_started.elapsed().as_nanos();
    let after_history = opened.fs.counter_snapshot().map_err(display_error)?;
    let after = opened.fs.diagnostics().map_err(display_error)?;
    let engine_start = Diagnostics::default();
    let engine = EngineDelta::between(&engine_start, &after)?;
    engine.verify_read_only()?;
    let open_engine = PhaseCounterDelta::between("verified_open", &engine_start, &after_open)?;
    open_engine.engine.verify_read_only()?;
    let storage_before = PhaseCounterDelta::between("storage_observation", &after_open, &before)?;
    storage_before.engine.verify_read_only()?;
    let history_engine = PhaseCounterDelta::between("history_read", &before, &after_history)?;
    history_engine.engine.verify_read_only()?;
    let storage_after = PhaseCounterDelta::between("storage_observation", &after_history, &after)?;
    storage_after.engine.verify_read_only()?;
    let probe_engine = history_probes
        .iter()
        .try_fold(EngineDelta::default(), |aggregate, probe| {
            aggregate.combine(probe.engine)
        })?;
    let probe_operation = history_probes.iter().try_fold(
        crate::legacy_full::OperationDiagnostics::default(),
        |aggregate, probe| aggregate.merge(probe.operation).map_err(display_error),
    )?;
    if probe_engine != history_engine.engine
        || counters_json(Some(probe_engine), Some(&probe_operation))?
            != counters_json(Some(history_engine.engine), Some(&operation))?
    {
        return Err(format!(
            "history session {session} probe counters sum to retained row"
        ));
    }
    let phase_counters = vec![open_engine, storage_before, history_engine, storage_after];
    verify_phase_partition(&phase_counters, engine)?;
    if operation.native.bytes_read != 0
        || operation.native.bytes_written != 0
        || content_counters(&operation)?.cdc_bytes_scanned != 0
    {
        return Err(format!("history session {session} uses no native/CDC work"));
    }
    let active_connections = after
        .active_connections
        .checked_add(
            retained
                .counter_snapshot()
                .map_err(display_error)?
                .active_connections,
        )
        .ok_or_else(|| "history active connection count overflow".to_owned())?;
    let resources = if session == 1 {
        let external = observe_external_resources(Some(work), Some(store))?;
        if external.active_store_connections != active_connections {
            return Err("history SDK/external active connection equality".to_owned());
        }
        external
    } else {
        observe_row_resources(Some(work), active_connections)?
    };
    drop(opened);
    let row_wall = row_started.elapsed().as_nanos();
    let phases = vec![
        Phase {
            name: "verified_open",
            wall_ns: verified_wall,
        },
        Phase {
            name: "history_read",
            wall_ns: history_wall,
        },
    ];
    campaign.append(RowReceipt {
        schedule,
        status: "PASS",
        before_bytes: snapshots[head_index].logical_length,
        after_bytes: snapshots[head_index].logical_length,
        edit: None,
        sub_edits: Vec::new(),
        history_probes,
        pre_ref: Some(roots[head_index].clone()),
        post_ref: Some(roots[head_index].clone()),
        native_route: "NotApplicable".to_owned(),
        tree_level_before: None,
        phases: phases.clone(),
        phase_counters,
        row_wall_ns: row_wall,
        row_residual_ns: row_residual(row_wall, &phases)?,
        engine: Some(engine),
        operation: Some(operation),
        storage_before: Some(before),
        storage_after: Some(after),
        resources,
        oracle: OracleReceipt {
            logical_length: snapshots[head_index].logical_length,
            content_digest: digest,
            canonical_bytes_exact: Some(true),
            historical_roots_exact: Some(true),
            route_exact: Some(true),
            ..OracleReceipt::default()
        },
        unavailable: unavailable_defaults(),
        error: None,
        custody: Some(history_custody_json(session)?),
    })
}
