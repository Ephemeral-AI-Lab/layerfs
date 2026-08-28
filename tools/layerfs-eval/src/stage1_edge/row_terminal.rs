use super::artifact::{
    display_error, io_error, json_bool, json_escape, json_string, json_u128, sha256_bytes,
    sha256_file,
};
use super::campaign::Campaign;
use super::context::set_failure_phase;
use super::engine_counters::{EngineDelta, FixtureMaster, PhaseCounterDelta};
use super::fixture::{fixture_root, verify_fixture, SourceIdentity};
use super::limits::{CAMPAIGN_LIMIT_NS, INITIAL_BYTES, READINESS_SCHEMA, RESET_LIMIT_NS};
use super::receipt_model::{OracleReceipt, Phase, RowReceipt, Unavailable};
use super::resources::{observe_external_resources, row_residual, unavailable_defaults};
use super::row_milestone::terminal_work_residue_count;
use crate::legacy_full::LayerFs;
use crate::stage1_fixture::{self, EvalResult};
use std::fs;
use std::path::Path;
use std::time::Instant;
pub(crate) fn run_terminal_row(
    campaign: &mut Campaign<'_>,
    fs: &LayerFs,
    mut converted: Option<crate::legacy_full::ExternalWorkspace>,
    work: &Path,
    fixture: &Path,
    master: &FixtureMaster,
) -> EvalResult<()> {
    let schedule = campaign.scheduled("C09-001")?;
    let row_started = Instant::now();
    set_failure_phase("explicit_cleanup");
    let cleanup_started = Instant::now();
    let cleanup = converted.as_mut().map_or_else(
        || Ok(crate::legacy_full::OperationDiagnostics::default()),
        |external| external.discard_observed().map_err(display_error),
    )?;
    drop(converted);
    fs.close_primary_connections().map_err(display_error)?;
    let store = work.join("store");
    let mut before_cleanup = observe_external_resources(Some(work), Some(&store))?;
    before_cleanup.residue_entries = before_cleanup
        .residue_entries
        .checked_add(terminal_work_residue_count(work)?)
        .ok_or_else(|| "terminal residue count overflow".to_owned())?;
    if before_cleanup.active_store_connections != 0
        || before_cleanup.fd_current != campaign.fd_baseline
        || before_cleanup.child_processes != 0
        || before_cleanup.residue_entries != 0
    {
        return Err(format!(
            concat!(
                "pre-deletion terminal closure connections={} fd={}/{} ",
                "children={} residue={}"
            ),
            before_cleanup.active_store_connections,
            campaign.fd_baseline,
            before_cleanup.fd_current,
            before_cleanup.child_processes,
            before_cleanup.residue_entries,
        ));
    }
    if work.exists() {
        stage1_fixture::make_writable(work)?;
        fs::remove_dir_all(work).map_err(io_error)?;
    }
    verify_fixture(fixture, master, true)?;
    let cleanup_wall = cleanup_started.elapsed().as_nanos();
    let mut resources = observe_external_resources(Some(work), None)?;
    resources.owned_temp_entries = Some(0);
    if campaign.rss_peak_bytes.max(resources.rss_peak_bytes) > 33_554_432
        || campaign.q_high_water_bytes > 8_388_608
        || campaign.q_maximum_terminal_bytes != 0
        || campaign.store_connection_high_water > 2
        || resources.active_store_connections != 0
        || resources.fd_current != campaign.fd_baseline
        || resources.child_processes != 0
        || resources.residue_entries != 0
    {
        return Err(format!(
            concat!(
                "terminal resource closure rss={} q={}/{} connections={}/{} ",
                "fd={}/{} children={} residue={}"
            ),
            campaign.rss_peak_bytes.max(resources.rss_peak_bytes),
            campaign.q_high_water_bytes,
            campaign.q_maximum_terminal_bytes,
            campaign.store_connection_high_water,
            resources.active_store_connections,
            campaign.fd_baseline,
            resources.fd_current,
            resources.child_processes,
            resources.residue_entries,
        ));
    }
    let row_wall = row_started.elapsed().as_nanos();
    let phases = vec![Phase {
        name: "explicit_cleanup",
        wall_ns: cleanup_wall,
    }];
    let phase_counters = vec![PhaseCounterDelta::operation_only(
        "explicit_cleanup",
        &cleanup,
        0,
    )];
    campaign.append(RowReceipt {
        schedule,
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
        phases: phases.clone(),
        phase_counters,
        row_wall_ns: row_wall,
        row_residual_ns: row_residual(row_wall, &phases)?,
        engine: Some(EngineDelta::default()),
        operation: Some(cleanup),
        storage_before: None,
        storage_after: None,
        resources,
        oracle: OracleReceipt {
            logical_length: INITIAL_BYTES,
            content_digest: String::new(),
            ..OracleReceipt::default()
        },
        unavailable: {
            let mut unavailable = unavailable_defaults();
            unavailable.push(Unavailable {
                field: "oracle.content_digest".to_owned(),
                availability: "NotApplicable",
                reason: "workspace authority was discarded before terminal resource observation"
                    .to_owned(),
            });
            unavailable
        },
        error: None,
        custody: Some(format!(
            concat!(
                "{{\"pre_cleanup_active_store_connections\":{},",
                "\"pre_cleanup_fd_count\":{},\"pre_cleanup_child_processes\":{},",
                "\"pre_cleanup_residue_entries\":{},",
                "\"post_cleanup_active_store_connections\":{},",
                "\"post_cleanup_fd_count\":{},\"post_cleanup_child_processes\":{},",
                "\"post_cleanup_residue_entries\":{},",
                "\"fixture_unchanged\":true}}"
            ),
            before_cleanup.active_store_connections,
            before_cleanup.fd_current,
            before_cleanup.child_processes,
            before_cleanup.residue_entries,
            resources.active_store_connections,
            resources.fd_current,
            resources.child_processes,
            resources.residue_entries,
        )),
    })
}
pub(crate) fn admit_readiness(
    json: &str,
    source: &SourceIdentity,
    master: &FixtureMaster,
    schedule: &str,
) -> EvalResult<()> {
    if json_string(json, "schema")? != READINESS_SCHEMA
        || json_string(json, "status")? != "PASS"
        || json_bool(json, "measured_rows_started")?
        || json_bool(json, "run_directory_exists")?
        || json_u128(json, "expected_rows")? != 47
        || json_u128(json, "edit_suboperations")? != 51
        || json_u128(json, "transitions")? != 34
        || json_u128(json, "reset_wall_ns")? > RESET_LIMIT_NS
        || json_u128(json, "forecast_campaign_wall_ns")? >= CAMPAIGN_LIMIT_NS
        || json_u128(json, "hard_limit_ns")? != CAMPAIGN_LIMIT_NS
        || json_string(json, "source_tree_blake3")? != source.tree_blake3
        || json_string(json, "source_manifest_sha256")? != source.manifest_sha256
        || json_string(json, "executable_path")? != source.executable_path.display().to_string()
        || json_string(json, "executable_sha256")? != source.executable_sha256
        || json_string(json, "executable_blake3")? != source.executable_blake3
        || json_string(json, "fixture_master_sha256")?
            != sha256_file(&fixture_root().join("master.json"))?
        || json_string(json, "fixture_blake3")? != master.fixture_blake3
        || json_string(json, "schedule_sha256")? != sha256_bytes(schedule.as_bytes())?
        || json_string(json, "store_id")? != master.store_id
        || json_string(json, "profile")? != master.profile
        || json_string(json, "apfs_identity")? != master.apfs_identity
        || json_string(json, "git_commit")? != source.git_commit
        || json_bool(json, "dirty_tree")? != source.dirty_tree
    {
        return Err(
            "readiness receipt does not bind this exact source/executable/fixture/schedule"
                .to_owned(),
        );
    }
    Ok(())
}
pub(crate) fn environment_json(source: &SourceIdentity, master: &FixtureMaster) -> String {
    format!(
        concat!(
            "{{\"schema\":\"layerfs-stage1.1-environment-v1\",",
            "\"git_commit\":\"{}\",\"dirty_tree\":{},",
            "\"source_tree_blake3\":\"{}\",\"source_manifest_sha256\":\"{}\",",
            "\"release_executable_path\":\"{}\",",
            "\"release_executable_sha256\":\"{}\",",
            "\"release_executable_blake3\":\"{}\",",
            "\"apfs_identity\":\"{}\",\"store_id\":\"{}\",",
            "\"profile\":\"{}\",\"network_operations\":0,",
            "\"product_operation_child_processes\":0,",
            "\"command\":\"layerfs-eval stage1 run apple-edge <new-run-directory>\"}}\n"
        ),
        source.git_commit,
        source.dirty_tree,
        source.tree_blake3,
        source.manifest_sha256,
        json_escape(&source.executable_path.display().to_string()),
        source.executable_sha256,
        source.executable_blake3,
        json_escape(&master.apfs_identity),
        master.store_id,
        json_escape(&master.profile),
    )
}
