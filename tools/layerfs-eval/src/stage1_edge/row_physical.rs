use super::artifact::display_error;
use super::campaign::Campaign;
use super::context::set_failure_phase;
use super::engine_counters::{verify_phase_partition, EngineDelta, PhaseCounterDelta};
use super::limits::FILE_PATH;
use super::locality::verify_locality;
use super::oracle::{
    combine_physical_checkpoint, compare_canonical, compare_managed, native_route_name,
    verify_native_edit, verify_storage_transition, verify_supported_metadata,
};
use super::receipt_model::{OracleReceipt, Phase, RowReceipt};
use super::resources::{observe_row_resources, row_residual, unavailable_defaults};
use super::schedule_model::{EditSpec, PieceTable};
use crate::legacy_full::{LayerFs, NativeMetadata, RefState};
use crate::stage1_fixture::EvalResult;
use std::path::Path;
use std::time::Instant;
#[allow(clippy::too_many_arguments)]
pub(crate) fn run_physical_row(
    campaign: &mut Campaign<'_>,
    fs: &LayerFs,
    managed: &mut crate::legacy_full::ManagedWorkspace,
    edit: &EditSpec,
    table: &PieceTable,
    roots: &mut Vec<RefState>,
    metadata: &mut Vec<NativeMetadata>,
    work: &Path,
) -> EvalResult<()> {
    let id = format!("C03-{:03}", edit.serial);
    let schedule = campaign.scheduled(&id)?;
    let row_started = Instant::now();
    let pre_ref = roots
        .last()
        .cloned()
        .ok_or_else(|| "physical transition has no pre-ref".to_owned())?;
    let before_storage = fs.diagnostics().map_err(display_error)?;
    let replacement = campaign
        .schedule
        .replacement_backing
        .get(
            edit.replacement_offset
                ..edit
                    .replacement_offset
                    .checked_add(usize::try_from(edit.insert_bytes).map_err(display_error)?)
                    .ok_or_else(|| "physical replacement range overflow".to_owned())?,
        )
        .ok_or_else(|| "physical replacement exceeds backing".to_owned())?;
    set_failure_phase("native_edit");
    let native_started = Instant::now();
    let native = managed
        .replace_observed(FILE_PATH, edit.offset, edit.delete_bytes, replacement)
        .map_err(display_error)?;
    let native_wall = native_started.elapsed().as_nanos();
    verify_native_edit(edit, &native)?;
    set_failure_phase("live_physical_oracle");
    let physical_started = Instant::now();
    let (physical_digest, _) =
        compare_managed(managed, table, &campaign.schedule.replacement_backing)?;
    let current_metadata = managed.read_metadata(FILE_PATH).map_err(display_error)?;
    verify_supported_metadata(&current_metadata, &edit.tag)?;
    let physical_wall = physical_started.elapsed().as_nanos();
    campaign.physical_oracles += 1;
    set_failure_phase("durable_checkpoint");
    let checkpoint_started = Instant::now();
    let (post_ref, checkpoint) = managed.checkpoint_observed().map_err(display_error)?;
    let checkpoint_wall = checkpoint_started.elapsed().as_nanos();
    if post_ref.name != "main"
        || post_ref.generation != pre_ref.generation + 1
        || fs.current_head("main").map_err(display_error)? != post_ref
        || checkpoint.workspace_reuses != 1
        || checkpoint.rematerializations != 0
        || checkpoint.descriptor_resets != 1
        || checkpoint.operation_q_terminal_bytes != 0
    {
        return Err(format!("{} exact RefState/checkpoint closure", edit.tag));
    }
    let tree_level = checkpoint
        .rope
        .tree_level_before
        .ok_or_else(|| format!("{} checkpoint missing actual H", edit.tag))?;
    verify_locality(&checkpoint, edit.insert_bytes, tree_level)?;
    let after_checkpoint = fs.counter_snapshot().map_err(display_error)?;
    set_failure_phase("canonical_witness");
    let canonical_started = Instant::now();
    let (canonical_digest, _) = compare_canonical(
        fs,
        post_ref.root,
        table,
        &campaign.schedule.replacement_backing,
    )?;
    let canonical_wall = canonical_started.elapsed().as_nanos();
    if canonical_digest != physical_digest {
        return Err(format!("{} physical digest = canonical digest", edit.tag));
    }
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
            &native,
            before_storage.active_connections,
        ),
        checkpoint_engine,
        witness_engine,
        storage_engine,
    ];
    verify_phase_partition(&phase_counters, engine)?;
    let operation = combine_physical_checkpoint(native, checkpoint)?;
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
        before_bytes: edit.before_bytes,
        after_bytes: edit.after_bytes,
        edit: Some(edit.clone()),
        sub_edits: Vec::new(),
        history_probes: Vec::new(),
        pre_ref: Some(pre_ref),
        post_ref: Some(post_ref),
        native_route: native_route_name(operation.native.route).to_owned(),
        tree_level_before: Some(tree_level),
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
            logical_length: edit.after_bytes,
            content_digest: canonical_digest,
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
pub(crate) fn history_root_indices(session: u8) -> EvalResult<&'static [usize]> {
    match session {
        1 => Ok(&[0, 5]),
        2 => Ok(&[0, 5, 10]),
        3 => Ok(&[0, 5, 10, 15]),
        4 => Ok(&[0, 15, 20]),
        5 => Ok(&[0, 15, 20, 25]),
        6 => Ok(&[0, 15, 20, 25, 30]),
        _ => Err(format!("invalid history session {session}")),
    }
}
pub(crate) fn history_custody_json(session: u8) -> EvalResult<String> {
    let indices = history_root_indices(session)?;
    Ok(format!(
        "{{\"head\":\"R{}\",\"roots\":[{}]}}",
        usize::from(session) * 5,
        indices
            .iter()
            .map(|root| format!("\"R{root}\""))
            .collect::<Vec<_>>()
            .join(",")
    ))
}
