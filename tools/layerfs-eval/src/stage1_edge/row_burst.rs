use super::artifact::display_error;
use super::campaign::Campaign;
use super::context::set_failure_phase;
use super::engine_counters::{verify_phase_partition, EngineDelta, PhaseCounterDelta};
use super::limits::FILE_PATH;
use super::locality::{verify_burst_locality, verify_locality};
use super::oracle::{
    combine_physical_checkpoint, compare_canonical, compare_managed, native_route_name,
    verify_native_edit, verify_storage_transition, verify_supported_metadata,
};
use super::receipt_model::{OracleReceipt, Phase, RowReceipt, SubEditReceipt};
use super::resources::{observe_row_resources, row_residual, unavailable_defaults};
use super::schedule_model::{BurstSpec, PieceTable};
use crate::legacy_full::{LayerFs, NativeMetadata, RefState};
use crate::stage1_fixture::EvalResult;
use std::path::Path;
use std::time::Instant;
#[allow(clippy::too_many_arguments)]
pub(crate) fn run_burst_row(
    campaign: &mut Campaign<'_>,
    fs: &LayerFs,
    managed: &mut crate::legacy_full::ManagedWorkspace,
    burst: &BurstSpec,
    before_table: &PieceTable,
    expected_table: &PieceTable,
    roots: &mut Vec<RefState>,
    metadata: &mut Vec<NativeMetadata>,
    work: &Path,
) -> EvalResult<()> {
    let id = format!("C07-{:03}", burst.root - 30);
    let schedule = campaign.scheduled(&id)?;
    let row_started = Instant::now();
    let pre_ref = roots
        .last()
        .cloned()
        .ok_or_else(|| "burst has no pre-ref".to_owned())?;
    let before_storage = fs.diagnostics().map_err(display_error)?;
    let mut table = before_table.clone();
    let mut native_aggregate = crate::legacy_full::OperationDiagnostics::default();
    let mut sub_edits = Vec::new();
    let mut native_wall = 0_u128;
    let mut physical_wall = 0_u128;
    for edit in &burst.edits {
        let replacement = campaign
            .schedule
            .replacement_backing
            .get(
                edit.replacement_offset
                    ..edit
                        .replacement_offset
                        .checked_add(usize::try_from(edit.insert_bytes).map_err(display_error)?)
                        .ok_or_else(|| "burst replacement range overflow".to_owned())?,
            )
            .ok_or_else(|| "burst replacement exceeds backing".to_owned())?;
        set_failure_phase("native_edit");
        let native_started = Instant::now();
        let native = managed
            .replace_observed(FILE_PATH, edit.offset, edit.delete_bytes, replacement)
            .map_err(display_error)?;
        let one_native_wall = native_started.elapsed().as_nanos();
        native_wall = native_wall
            .checked_add(one_native_wall)
            .ok_or_else(|| "burst native wall overflow".to_owned())?;
        verify_native_edit(edit, &native)?;
        table.splice(edit)?;
        set_failure_phase("live_physical_oracle");
        let oracle_started = Instant::now();
        compare_managed(managed, &table, &campaign.schedule.replacement_backing)?;
        let one_oracle_wall = oracle_started.elapsed().as_nanos();
        physical_wall = physical_wall
            .checked_add(one_oracle_wall)
            .ok_or_else(|| "burst oracle wall overflow".to_owned())?;
        campaign.physical_oracles += 1;
        sub_edits.push(SubEditReceipt {
            edit: edit.clone(),
            native_wall_ns: one_native_wall,
            physical_oracle_wall_ns: one_oracle_wall,
            native_route: native_route_name(native.native.route).to_owned(),
            native_bytes_read: native.native.bytes_read,
            native_bytes_written: native.native.bytes_written,
            native_patch_bytes: native.native.patch_bytes,
            native_suffix_bytes_shifted: native.native.suffix_bytes_shifted,
            native_clone_attempts: native.native.clone_attempts,
            native_clone_successes: native.native.clone_successes,
            native_clone_fallbacks: native.native.clone_fallbacks,
            native_full_fallback_files: native.full_fallback_files,
            tree_level_before: None,
            locality: None,
        });
        native_aggregate = native_aggregate.merge(native).map_err(display_error)?;
    }
    if &table != expected_table {
        return Err(format!("R{} ordered burst oracle table", burst.root));
    }
    let current_metadata = managed.read_metadata(FILE_PATH).map_err(display_error)?;
    verify_supported_metadata(&current_metadata, &format!("R{} burst", burst.root))?;
    set_failure_phase("durable_checkpoint");
    let checkpoint_started = Instant::now();
    let (post_ref, checkpoint, replay_steps) = managed
        .checkpoint_observed_detailed()
        .map_err(display_error)?;
    let checkpoint_wall = checkpoint_started.elapsed().as_nanos();
    if post_ref.generation != pre_ref.generation + 1
        || fs.current_head("main").map_err(display_error)? != post_ref
        || checkpoint.workspace_reuses != 1
        || checkpoint.rematerializations != 0
        || checkpoint.descriptor_resets != 1
    {
        return Err(format!("R{} burst checkpoint closure", burst.root));
    }
    if replay_steps.len() != burst.edits.len() {
        return Err(format!(
            "R{} replay step count {} != {}",
            burst.root,
            replay_steps.len(),
            burst.edits.len()
        ));
    }
    for ((edit, receipt), step) in burst
        .edits
        .iter()
        .zip(sub_edits.iter_mut())
        .zip(replay_steps.iter())
    {
        let step_level = step
            .tree_level_before
            .ok_or_else(|| format!("{} missing replay tree level", edit.tag))?;
        let locality = verify_locality(&step.counters, edit.insert_bytes, step_level)?;
        receipt.tree_level_before = Some(step_level);
        receipt.locality = Some(locality);
    }
    verify_burst_locality(&checkpoint, &burst.edits, &replay_steps)?;
    let after_checkpoint = fs.counter_snapshot().map_err(display_error)?;
    set_failure_phase("canonical_witness");
    let canonical_started = Instant::now();
    let (digest, _) = compare_canonical(
        fs,
        post_ref.root,
        expected_table,
        &campaign.schedule.replacement_backing,
    )?;
    let canonical_wall = canonical_started.elapsed().as_nanos();
    let after_witness = fs.counter_snapshot().map_err(display_error)?;
    campaign.canonical_transitions += 1;
    set_failure_phase("counter_snapshot");
    let counter_started = Instant::now();
    let after_storage = fs.diagnostics().map_err(display_error)?;
    verify_storage_transition(&before_storage, &after_storage)?;
    let engine = EngineDelta::between(&before_storage, &after_storage)?;
    engine.verify_trusted_transition()?;
    let checkpoint_engine =
        PhaseCounterDelta::between("checkpoint", &before_storage, &after_checkpoint)?;
    checkpoint_engine.engine.verify_trusted_transition()?;
    let witness_engine =
        PhaseCounterDelta::between("canonical_witness", &after_checkpoint, &after_witness)?;
    witness_engine.engine.verify_trusted_read_only()?;
    let storage_engine =
        PhaseCounterDelta::between("storage_observation", &after_witness, &after_storage)?;
    storage_engine.engine.verify_trusted_read_only()?;
    let phase_counters = vec![
        PhaseCounterDelta::operation_only(
            "native_edit",
            &native_aggregate,
            before_storage.active_connections,
        ),
        checkpoint_engine,
        witness_engine,
        storage_engine,
    ];
    verify_phase_partition(&phase_counters, engine)?;
    let operation = combine_physical_checkpoint(native_aggregate, checkpoint)?;
    let resources = observe_row_resources(Some(work), after_storage.active_connections)?;
    let counter_wall = counter_started.elapsed().as_nanos();
    let row_wall = row_started.elapsed().as_nanos();
    let phases = vec![
        Phase {
            name: "native_edit",
            wall_ns: native_wall,
        },
        Phase {
            name: "live_physical_oracle",
            wall_ns: physical_wall,
        },
        Phase {
            name: "durable_checkpoint",
            wall_ns: checkpoint_wall,
        },
        Phase {
            name: "canonical_witness",
            wall_ns: canonical_wall,
        },
        Phase {
            name: "counter_snapshot",
            wall_ns: counter_wall,
        },
    ];
    roots.push(post_ref.clone());
    metadata.push(current_metadata);
    campaign.append(RowReceipt {
        schedule,
        status: "PASS",
        before_bytes: burst.edits[0].before_bytes,
        after_bytes: burst
            .edits
            .last()
            .ok_or_else(|| "empty burst".to_owned())?
            .after_bytes,
        edit: None,
        sub_edits,
        history_probes: Vec::new(),
        pre_ref: Some(pre_ref),
        post_ref: Some(post_ref),
        native_route: "NotApplicable".to_owned(),
        tree_level_before: None,
        phases: phases.clone(),
        phase_counters,
        row_wall_ns: row_wall,
        row_residual_ns: row_residual(row_wall, &phases)?,
        engine: Some(engine),
        operation: Some(operation),
        storage_before: Some(before_storage),
        storage_after: Some(after_storage),
        resources,
        oracle: OracleReceipt {
            logical_length: expected_table.logical_length,
            content_digest: digest,
            physical_bytes_exact: Some(true),
            canonical_bytes_exact: Some(true),
            metadata_exact: Some(true),
            route_exact: Some(true),
            ..OracleReceipt::default()
        },
        unavailable: unavailable_defaults(),
        error: None,
        custody: None,
    })
}
