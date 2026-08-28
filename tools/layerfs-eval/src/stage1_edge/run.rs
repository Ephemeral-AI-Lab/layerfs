use super::artifact::{
    display_error, durable_write, hex, io_error, json_escape, sha256_bytes, sha256_file, unix_ns,
};
use super::campaign::Campaign;
use super::context::{begin_failure_context, set_failure_phase, Disposition};
use super::engine_counters::{verify_phase_partition, EngineDelta, PhaseCounterDelta};
use super::fixture::{fixture_root, read_master, readiness_path, verify_fixture};
use super::limits::{
    FILE_PATH, FIXTURE_MODE, FIXTURE_MTIME_NANOSECONDS, FIXTURE_MTIME_SECONDS, INITIAL_BYTES,
    RESET_LIMIT_NS,
};
use super::oracle::compare_managed;
use super::receipt_model::{OracleReceipt, Phase, RowReceipt};
use super::report_disposition::finalize_reports;
use super::resources::{
    fd_count, maximum_rss_bytes, observe_row_resources, row_residual, unavailable_defaults,
};
use super::row_burst::run_burst_row;
use super::row_history::run_history_row;
use super::row_logical::run_logical_row;
use super::row_milestone::run_milestone_row;
use super::row_physical::run_physical_row;
use super::row_terminal::{admit_readiness, environment_json, run_terminal_row};
use super::schedule::{frozen_schedule, oracle_snapshots};
use super::schedule_model::PieceTable;
use super::source_identity::{schedule_json, source_identity};
use crate::legacy_full::{Diagnostics, IntegrityMode, LayerFs};
use crate::stage1_fixture::{self, EvalResult};
use std::fs::{self, OpenOptions};
use std::path::Path;
use std::time::Instant;
pub(crate) fn run_inner(run: &Path) -> EvalResult<Disposition> {
    let started = Instant::now();
    let started_unix_ns = unix_ns()?;
    let schedule = frozen_schedule()?;
    let schedule_bytes = schedule_json(&schedule)?;
    let fixture = fixture_root();
    let master = read_master(&fixture)?;
    let source = source_identity()?;
    let readiness = fs::read_to_string(readiness_path()).map_err(io_error)?;
    admit_readiness(&readiness, &source, &master, &schedule_bytes)?;
    fs::create_dir(run).map_err(io_error)?;
    stage1_fixture::sync_directory(
        run.parent()
            .ok_or_else(|| "run directory has no parent".to_owned())?,
    )?;
    let run_directory = run.canonicalize().map_err(io_error)?;
    let run = run_directory.as_path();
    let rows = OpenOptions::new()
        .append(true)
        .create_new(true)
        .open(run.join("rows.jsonl"))
        .map_err(io_error)?;
    let fd_baseline = fd_count()?;
    let mut campaign = Campaign {
        run,
        started,
        started_unix_ns,
        rows,
        schedule: &schedule,
        next_row: 0,
        row_wall_sum_ns: 0,
        fd_baseline,
        rss_peak_bytes: maximum_rss_bytes()?,
        q_high_water_bytes: 0,
        q_maximum_terminal_bytes: 0,
        store_connection_high_water: 0,
        physical_oracles: 0,
        canonical_transitions: 0,
        workspace_materializations: 0,
        rematerializations: 0,
        root_digests: Vec::with_capacity(35),
    };
    begin_failure_context("C00-001", "admission");
    let c00_started = Instant::now();
    let admission_started = Instant::now();
    verify_fixture(&fixture, &master, true)?;
    durable_write(
        &run.join("environment.json"),
        &environment_json(&source, &master),
    )?;
    durable_write(
        &run.join("master.json"),
        &fs::read_to_string(fixture.join("master.json")).map_err(io_error)?,
    )?;
    durable_write(&run.join("readiness.json"), &readiness)?;
    durable_write(&run.join("schedule.json"), &schedule_bytes)?;
    let admission_wall = admission_started.elapsed().as_nanos();
    let c00_resources = observe_row_resources(Some(&fixture), 0)?;
    let c00_wall = c00_started.elapsed().as_nanos();
    let c00_phases = vec![Phase {
        name: "admission",
        wall_ns: admission_wall,
    }];
    let custody = format!(
        concat!(
            "{{\"git_commit\":\"{}\",\"dirty_tree\":{},",
            "\"source_tree_blake3\":\"{}\",\"source_manifest_sha256\":\"{}\",",
            "\"executable_path\":\"{}\",\"executable_sha256\":\"{}\",",
            "\"executable_blake3\":\"{}\",\"fixture_blake3\":\"{}\",",
            "\"fixture_master_sha256\":\"{}\",\"readiness_sha256\":\"{}\",",
            "\"schedule_sha256\":\"{}\",\"apfs_identity\":\"{}\",",
            "\"store_id\":\"{}\"}}"
        ),
        source.git_commit,
        source.dirty_tree,
        source.tree_blake3,
        source.manifest_sha256,
        json_escape(&source.executable_path.display().to_string()),
        source.executable_sha256,
        source.executable_blake3,
        master.fixture_blake3,
        sha256_file(&fixture.join("master.json"))?,
        sha256_bytes(readiness.as_bytes())?,
        sha256_bytes(schedule_bytes.as_bytes())?,
        json_escape(&master.apfs_identity),
        master.store_id,
    );
    campaign.append(RowReceipt {
        schedule: campaign.scheduled("C00-001")?,
        status: "PASS",
        before_bytes: INITIAL_BYTES,
        after_bytes: INITIAL_BYTES,
        edit: None,
        sub_edits: Vec::new(),
        history_probes: Vec::new(),
        pre_ref: None,
        post_ref: None,
        native_route: "NotApplicable".to_owned(),
        tree_level_before: None,
        phases: c00_phases.clone(),
        phase_counters: Vec::new(),
        row_wall_ns: c00_wall,
        row_residual_ns: row_residual(c00_wall, &c00_phases)?,
        engine: None,
        operation: None,
        storage_before: None,
        storage_after: None,
        resources: c00_resources,
        oracle: OracleReceipt {
            logical_length: INITIAL_BYTES,
            content_digest: master.raw_digest.clone(),
            canonical_bytes_exact: Some(true),
            route_exact: Some(true),
            ..OracleReceipt::default()
        },
        unavailable: unavailable_defaults(),
        error: None,
        custody: Some(custody),
    })?;
    let work = run.join(".work");
    fs::create_dir(&work).map_err(io_error)?;
    let store = work.join("store");
    begin_failure_context("C01-001", "reset");
    let c01_started = Instant::now();
    let reset_started = Instant::now();
    stage1_fixture::clone_directory(&fixture.join("bases/base"), &store)?;
    stage1_fixture::make_writable(&store)?;
    let reset_wall = reset_started.elapsed().as_nanos();
    if reset_wall > RESET_LIMIT_NS {
        return Err("reset_wall_ns <= 5,000,000,000".to_owned());
    }
    let c01_resources = observe_row_resources(Some(&work), 0)?;
    let c01_wall = c01_started.elapsed().as_nanos();
    let c01_phases = vec![Phase {
        name: "reset",
        wall_ns: reset_wall,
    }];
    campaign.append(RowReceipt {
        schedule: campaign.scheduled("C01-001")?,
        status: "PASS",
        before_bytes: INITIAL_BYTES,
        after_bytes: INITIAL_BYTES,
        edit: None,
        sub_edits: Vec::new(),
        history_probes: Vec::new(),
        pre_ref: None,
        post_ref: None,
        native_route: "NotApplicable".to_owned(),
        tree_level_before: None,
        phases: c01_phases.clone(),
        phase_counters: Vec::new(),
        row_wall_ns: c01_wall,
        row_residual_ns: row_residual(c01_wall, &c01_phases)?,
        engine: None,
        operation: None,
        storage_before: None,
        storage_after: None,
        resources: c01_resources,
        oracle: OracleReceipt {
            logical_length: INITIAL_BYTES,
            content_digest: master.raw_digest.clone(),
            route_exact: Some(true),
            ..OracleReceipt::default()
        },
        unavailable: unavailable_defaults(),
        error: None,
        custody: None,
    })?;
    begin_failure_context("C02-001", "store_open");
    let c02_started = Instant::now();
    let open_started = Instant::now();
    let opened = LayerFs::open_with_integrity(&store, IntegrityMode::TrustedLocalDev)
        .map_err(display_error)?;
    if opened.ref_state.root != master.root
        || opened.ref_state.generation != master.generation
        || hex(&opened.fs.store_id().map_err(display_error)?) != master.store_id
    {
        return Err("reset Store RefState/StoreId custody".to_owned());
    }
    let after_open = opened.fs.counter_snapshot().map_err(display_error)?;
    let store_open_wall = open_started.elapsed().as_nanos();
    let before_c02 = opened.fs.diagnostics().map_err(display_error)?;
    set_failure_phase("cold_materialization");
    let materialize_started = Instant::now();
    let (managed, materialize) = opened
        .fs
        .materialize_managed_observed(master.root)
        .map_err(display_error)?;
    let materialize_wall = materialize_started.elapsed().as_nanos();
    if materialize.workspace_materializations != 1
        || materialize.rematerializations != 0
        || materialize.operation_q_terminal_bytes != 0
    {
        return Err("initial managed materialization 1/0/Q=0".to_owned());
    }
    let initial_table = PieceTable::initial();
    set_failure_phase("live_physical_oracle");
    let oracle_started = Instant::now();
    let (initial_digest, _) =
        compare_managed(&managed, &initial_table, &schedule.replacement_backing)?;
    let oracle_wall = oracle_started.elapsed().as_nanos();
    if initial_digest != master.raw_digest {
        return Err("initial managed materialization digest".to_owned());
    }
    let initial_metadata = managed.read_metadata(FILE_PATH).map_err(display_error)?;
    if initial_metadata.mode != FIXTURE_MODE
        || initial_metadata.mtime_seconds
            != i64::try_from(FIXTURE_MTIME_SECONDS).map_err(display_error)?
        || initial_metadata.mtime_nanoseconds != FIXTURE_MTIME_NANOSECONDS
        || !initial_metadata.xattrs.is_empty()
        || initial_metadata.acl.is_some()
        || initial_metadata.bsd_flags != 0
    {
        return Err("initial exact Apple metadata".to_owned());
    }
    set_failure_phase("counter_snapshot");
    let after_materialize = opened.fs.counter_snapshot().map_err(display_error)?;
    let after_c02 = opened.fs.diagnostics().map_err(display_error)?;
    let engine_start = Diagnostics::default();
    let c02_engine = EngineDelta::between(&engine_start, &after_c02)?;
    c02_engine.verify_trusted_read_only()?;
    let open_engine = PhaseCounterDelta::between("store_open", &engine_start, &after_open)?;
    open_engine.engine.verify_trusted_read_only()?;
    let storage_before =
        PhaseCounterDelta::between("storage_observation", &after_open, &before_c02)?;
    storage_before.engine.verify_trusted_read_only()?;
    let materialize_engine =
        PhaseCounterDelta::between("materialization", &before_c02, &after_materialize)?
            .with_operation_scratch(&materialize);
    materialize_engine.engine.verify_trusted_read_only()?;
    let storage_after =
        PhaseCounterDelta::between("storage_observation", &after_materialize, &after_c02)?;
    storage_after.engine.verify_trusted_read_only()?;
    let phase_counters = vec![
        open_engine,
        storage_before,
        materialize_engine,
        storage_after,
    ];
    verify_phase_partition(&phase_counters, c02_engine)?;
    let c02_resources = observe_row_resources(Some(&work), after_c02.active_connections)?;
    let c02_wall = c02_started.elapsed().as_nanos();
    let c02_phases = vec![
        Phase {
            name: "store_open",
            wall_ns: store_open_wall,
        },
        Phase {
            name: "cold_materialization",
            wall_ns: materialize_wall,
        },
        Phase {
            name: "live_physical_oracle",
            wall_ns: oracle_wall,
        },
    ];
    campaign.append(RowReceipt {
        schedule: campaign.scheduled("C02-001")?,
        status: "PASS",
        before_bytes: INITIAL_BYTES,
        after_bytes: INITIAL_BYTES,
        edit: None,
        sub_edits: Vec::new(),
        history_probes: Vec::new(),
        pre_ref: Some(opened.ref_state.clone()),
        post_ref: Some(opened.ref_state.clone()),
        native_route: "NotApplicable".to_owned(),
        tree_level_before: None,
        phases: c02_phases.clone(),
        phase_counters,
        row_wall_ns: c02_wall,
        row_residual_ns: row_residual(c02_wall, &c02_phases)?,
        engine: Some(c02_engine),
        operation: Some(materialize),
        storage_before: Some(before_c02),
        storage_after: Some(after_c02),
        resources: c02_resources,
        oracle: OracleReceipt {
            logical_length: INITIAL_BYTES,
            content_digest: initial_digest,
            physical_bytes_exact: Some(true),
            canonical_bytes_exact: Some(true),
            metadata_exact: Some(true),
            route_exact: Some(true),
            ..OracleReceipt::default()
        },
        unavailable: unavailable_defaults(),
        error: None,
        custody: None,
    })?;
    let snapshots = oracle_snapshots(&schedule)?;
    let mut roots = vec![opened.ref_state.clone()];
    let mut metadata = vec![initial_metadata];
    let mut managed = Some(managed);
    for epoch in 0..3 {
        for within in 0..5 {
            let index = epoch * 5 + within;
            run_physical_row(
                &mut campaign,
                &opened.fs,
                managed
                    .as_mut()
                    .ok_or_else(|| "managed workspace already converted".to_owned())?,
                &schedule.edits[index],
                &snapshots[index + 1],
                &mut roots,
                &mut metadata,
                &work,
            )?;
        }
        run_history_row(
            &mut campaign,
            &opened.fs,
            &store,
            &roots,
            &snapshots,
            &schedule.replacement_backing,
            u8::try_from(epoch + 1).map_err(display_error)?,
            &work,
        )?;
    }
    for epoch in 0..3 {
        for within in 0..5 {
            let index = 15 + epoch * 5 + within;
            run_logical_row(
                &mut campaign,
                &opened.fs,
                managed
                    .as_mut()
                    .ok_or_else(|| "managed workspace already converted".to_owned())?,
                &schedule.edits[index],
                &snapshots[index + 1],
                &mut roots,
                &mut metadata,
                &work,
            )?;
        }
        run_history_row(
            &mut campaign,
            &opened.fs,
            &store,
            &roots,
            &snapshots,
            &schedule.replacement_backing,
            u8::try_from(epoch + 4).map_err(display_error)?,
            &work,
        )?;
    }
    for index in 0..4 {
        run_burst_row(
            &mut campaign,
            &opened.fs,
            managed
                .as_mut()
                .ok_or_else(|| "managed workspace already converted".to_owned())?,
            &schedule.bursts[index],
            &snapshots[30 + index],
            &snapshots[31 + index],
            &mut roots,
            &mut metadata,
            &work,
        )?;
    }
    let mut converted = None;
    for root in [15_u8, 30, 34] {
        run_milestone_row(
            &mut campaign,
            &opened.fs,
            &store,
            root,
            &roots,
            &metadata,
            &snapshots,
            &schedule.replacement_backing,
            &mut managed,
            &mut converted,
            &work,
        )?;
    }
    run_terminal_row(
        &mut campaign,
        &opened.fs,
        converted,
        &work,
        &fixture,
        &master,
    )?;
    if campaign.next_row != 47
        || campaign.physical_oracles != 51
        || campaign.canonical_transitions != 34
        || campaign.workspace_materializations != 1
        || campaign.rematerializations != 0
    {
        return Err(format!(
            "terminal population rows={} physical={} canonical={} materializations={} rematerializations={}",
            campaign.next_row,
            campaign.physical_oracles,
            campaign.canonical_transitions,
            campaign.workspace_materializations,
            campaign.rematerializations
        ));
    }
    campaign.rows.sync_all().map_err(io_error)?;
    begin_failure_context("__between_rows__", "report_validation");
    let disposition = finalize_reports(&mut campaign, &source, &master, &schedule)?;
    println!(
        "stage1.1-run status={} run={} rows=47 operations=51 transitions=34 wall_ns={}",
        disposition.as_str(),
        run.display(),
        campaign.started.elapsed().as_nanos()
    );
    Ok(disposition)
}
