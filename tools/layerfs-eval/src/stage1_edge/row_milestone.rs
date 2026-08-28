use super::artifact::{display_error, io_error};
use super::campaign::Campaign;
use super::context::set_failure_phase;
use super::engine_counters::{verify_phase_partition, EngineDelta, PhaseCounterDelta};
use super::limits::FILE_PATH;
use super::oracle::{
    compare_external, metadata_exact, metadata_receipt_json, verify_supported_metadata,
};
use super::receipt_model::{OracleReceipt, Phase, RowReceipt};
use super::resources::{observe_row_resources, row_residual, unavailable_defaults};
use super::schedule_model::PieceTable;
use crate::legacy_full::{Diagnostics, LayerFs, NativeMetadata, RefState};
use crate::stage1_fixture::EvalResult;
use std::fs;
use std::path::Path;
use std::time::Instant;
#[allow(clippy::too_many_arguments)]
pub(crate) fn run_milestone_row(
    campaign: &mut Campaign<'_>,
    retained: &LayerFs,
    store: &Path,
    root_index: u8,
    roots: &[RefState],
    metadata: &[NativeMetadata],
    snapshots: &[PieceTable],
    backing: &[u8],
    managed: &mut Option<crate::legacy_full::ManagedWorkspace>,
    converted: &mut Option<crate::legacy_full::ExternalWorkspace>,
    work: &Path,
) -> EvalResult<()> {
    let ordinal = match root_index {
        15 => 1,
        30 => 2,
        34 => 3,
        _ => return Err(format!("invalid materialization root R{root_index}")),
    };
    let id = format!("C08-{ordinal:03}");
    let schedule = campaign.scheduled(&id)?;
    let row_started = Instant::now();
    let mut phases = Vec::new();
    let mut live_digest = None;
    let mut live_extra_user_files = None;
    let mut live_metadata_receipt = None;
    if root_index == 34 {
        set_failure_phase("live_physical_oracle");
        let live_started = Instant::now();
        let external = managed
            .take()
            .ok_or_else(|| "R34 managed workspace already converted".to_owned())?
            .into_external()
            .map_err(display_error)?;
        verify_single_file_destination(external.path())?;
        live_extra_user_files = Some(0_u8);
        let digest = compare_external(&external, &snapshots[34], backing)?;
        let live_metadata = external.read_metadata(FILE_PATH).map_err(display_error)?;
        verify_supported_metadata(&live_metadata, "live R34")?;
        if !metadata_exact(&live_metadata, &metadata[34]) {
            return Err("live R34 metadata = retained R34 metadata".to_owned());
        }
        live_metadata_receipt = Some(live_metadata);
        live_digest = Some(digest);
        *converted = Some(external);
        phases.push(Phase {
            name: "live_physical_oracle",
            wall_ns: live_started.elapsed().as_nanos(),
        });
    }
    set_failure_phase("verified_open");
    let verified_started = Instant::now();
    let opened = LayerFs::open(store).map_err(display_error)?;
    let verified_wall = verified_started.elapsed().as_nanos();
    let after_open = opened.fs.counter_snapshot().map_err(display_error)?;
    phases.push(Phase {
        name: "verified_open",
        wall_ns: verified_wall,
    });
    let before = opened.fs.diagnostics().map_err(display_error)?;
    let destination = work.join(format!("milestone-R{root_index}"));
    set_failure_phase("milestone_materialization");
    let materialize_started = Instant::now();
    let (mut external, mut operation) = opened
        .fs
        .materialize_external_observed(roots[usize::from(root_index)].root, &destination)
        .map_err(display_error)?;
    let materialize_wall = materialize_started.elapsed().as_nanos();
    phases.push(Phase {
        name: "milestone_materialization",
        wall_ns: materialize_wall,
    });
    set_failure_phase("metadata_oracle");
    let oracle_started = Instant::now();
    let digest = compare_external(&external, &snapshots[usize::from(root_index)], backing)?;
    let actual_metadata = external.read_metadata(FILE_PATH).map_err(display_error)?;
    verify_supported_metadata(&actual_metadata, &format!("fresh R{root_index}"))?;
    if !metadata_exact(&actual_metadata, &metadata[usize::from(root_index)])
        || live_digest.as_ref().is_some_and(|live| live != &digest)
    {
        return Err(format!(
            "R{root_index} materialization byte/metadata oracle"
        ));
    }
    verify_single_file_destination(&destination)?;
    let oracle_wall = oracle_started.elapsed().as_nanos();
    phases.push(Phase {
        name: "metadata_oracle",
        wall_ns: oracle_wall,
    });
    let after_materialize = opened.fs.counter_snapshot().map_err(display_error)?;
    let after = opened.fs.diagnostics().map_err(display_error)?;
    set_failure_phase("explicit_cleanup");
    let cleanup_started = Instant::now();
    let cleanup = external.discard_observed().map_err(display_error)?;
    operation = operation.merge(cleanup).map_err(display_error)?;
    drop(external);
    fs::remove_dir_all(&destination).map_err(io_error)?;
    if destination.exists() {
        return Err(format!("R{root_index} milestone cleanup residue = 0"));
    }
    let cleanup_wall = cleanup_started.elapsed().as_nanos();
    phases.push(Phase {
        name: "explicit_cleanup",
        wall_ns: cleanup_wall,
    });
    let engine_start = Diagnostics::default();
    let engine = EngineDelta::between(&engine_start, &after)?;
    engine.verify_read_only()?;
    let open_engine = PhaseCounterDelta::between("verified_open", &engine_start, &after_open)?;
    open_engine.engine.verify_read_only()?;
    let storage_before = PhaseCounterDelta::between("storage_observation", &after_open, &before)?;
    storage_before.engine.verify_read_only()?;
    let materialize_engine =
        PhaseCounterDelta::between("materialization", &before, &after_materialize)?
            .with_operation_scratch(&operation);
    materialize_engine.engine.verify_read_only()?;
    let storage_after =
        PhaseCounterDelta::between("storage_observation", &after_materialize, &after)?;
    storage_after.engine.verify_read_only()?;
    let phase_counters = vec![
        open_engine,
        storage_before,
        materialize_engine,
        storage_after,
    ];
    verify_phase_partition(&phase_counters, engine)?;
    let active_connections = after
        .active_connections
        .checked_add(
            retained
                .counter_snapshot()
                .map_err(display_error)?
                .active_connections,
        )
        .ok_or_else(|| "milestone active connection count overflow".to_owned())?;
    let resources = observe_row_resources(Some(work), active_connections)?;
    drop(opened);
    let row_wall = row_started.elapsed().as_nanos();
    campaign.append(RowReceipt {
        schedule,
        status: "PASS",
        before_bytes: snapshots[usize::from(root_index)].logical_length,
        after_bytes: snapshots[usize::from(root_index)].logical_length,
        edit: None,
        sub_edits: Vec::new(),
        history_probes: Vec::new(),
        pre_ref: Some(roots[usize::from(root_index)].clone()),
        post_ref: Some(roots[usize::from(root_index)].clone()),
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
            logical_length: snapshots[usize::from(root_index)].logical_length,
            content_digest: digest,
            physical_bytes_exact: Some(true),
            canonical_bytes_exact: Some(true),
            metadata_exact: Some(true),
            historical_roots_exact: Some(true),
            route_exact: Some(true),
        },
        unavailable: unavailable_defaults(),
        error: None,
        custody: Some(format!(
            concat!(
                "{{\"milestone_root\":\"R{}\",",
                "\"extra_user_files\":0,\"fresh_extra_user_files\":0,",
                "\"live_extra_user_files\":{},\"cleanup_residue_entries\":0,",
                "\"metadata\":{},\"retained_metadata\":{},",
                "\"fresh_metadata\":{},\"live_metadata\":{}}}"
            ),
            root_index,
            live_extra_user_files.map_or_else(|| "null".to_owned(), |value| value.to_string()),
            metadata_receipt_json(&actual_metadata),
            metadata_receipt_json(&metadata[usize::from(root_index)]),
            metadata_receipt_json(&actual_metadata),
            live_metadata_receipt
                .as_ref()
                .map_or_else(|| "null".to_owned(), metadata_receipt_json,),
        )),
    })
}
pub(crate) fn verify_single_file_destination(destination: &Path) -> EvalResult<()> {
    let data = destination.join("data");
    let payload = data.join("payload.bin");
    if !data.is_dir() || !payload.is_file() {
        return Err("milestone destination is missing data/payload.bin".to_owned());
    }
    let root_entries = fs::read_dir(destination)
        .map_err(io_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(io_error)?;
    let data_entries = fs::read_dir(&data)
        .map_err(io_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(io_error)?;
    if root_entries.len() != 1
        || root_entries[0].file_name() != "data"
        || data_entries.len() != 1
        || data_entries[0].file_name() != "payload.bin"
        || fs::symlink_metadata(&payload)
            .map_err(io_error)?
            .file_type()
            .is_symlink()
    {
        return Err("milestone destination extra user files = 0".to_owned());
    }
    Ok(())
}
pub(crate) fn terminal_work_residue_count(work: &Path) -> EvalResult<u64> {
    let entries = fs::read_dir(work)
        .map_err(io_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(io_error)?;
    if !entries
        .iter()
        .any(|entry| entry.file_name() == "store" && entry.path().is_dir())
    {
        return Err("terminal work inventory is missing Store".to_owned());
    }
    Ok(entries
        .iter()
        .filter(|entry| entry.file_name() != "store")
        .count() as u64)
}
