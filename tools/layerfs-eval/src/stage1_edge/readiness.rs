use super::artifact::{
    display_error, durable_write, hex, io_error, json_escape, sha256_bytes, sha256_file, unix_ns,
};
use super::fixture::{fixture_root, read_master, readiness_path, validate_profile, verify_fixture};
use super::limits::{
    CAMPAIGN_LIMIT_NS, FROZEN_NON_RESET_FORECAST_NS, READINESS_SCHEMA, RESET_LIMIT_NS,
};
use super::schedule::frozen_schedule;
use super::source_identity::{schedule_json, source_identity};
use crate::legacy_full::{IntegrityMode, LayerFs};
use crate::stage1_fixture::{self, EvalResult};
use std::fs;
use std::time::Instant;
pub(crate) fn readiness() -> EvalResult<()> {
    if cfg!(debug_assertions) {
        return Err("Stage 1.1 readiness requires the release evaluator".to_owned());
    }
    let schedule = frozen_schedule()?;
    let schedule_json = schedule_json(&schedule)?;
    let root = fixture_root();
    let master = read_master(&root)?;
    verify_fixture(&root, &master, true)?;
    let source = source_identity()?;
    let reset = stage1_fixture::workspace_root().join(format!(
        "target/.layerfs-stage1.1-readiness-reset-{}-{}",
        std::process::id(),
        unix_ns()?
    ));
    let reset_started = Instant::now();
    stage1_fixture::clone_directory(&root.join("bases/base"), &reset)?;
    stage1_fixture::make_writable(&reset)?;
    let opened = LayerFs::open_with_integrity(&reset, IntegrityMode::TrustedLocalDev)
        .map_err(display_error)?;
    if opened.ref_state.root != master.root
        || opened.ref_state.generation != master.generation
        || hex(&opened.fs.store_id().map_err(display_error)?) != master.store_id
    {
        return Err("readiness reset authority mismatch".to_owned());
    }
    validate_profile(&opened.fs.counter_snapshot().map_err(display_error)?)?;
    drop(opened);
    stage1_fixture::make_writable(&reset)?;
    fs::remove_dir_all(&reset).map_err(io_error)?;
    let reset_wall_ns = reset_started.elapsed().as_nanos();
    if reset_wall_ns > RESET_LIMIT_NS {
        return Err(format!(
            "readiness reset {reset_wall_ns}ns exceeds {RESET_LIMIT_NS}ns"
        ));
    }
    let forecast = FROZEN_NON_RESET_FORECAST_NS
        .checked_add(reset_wall_ns)
        .ok_or_else(|| "readiness forecast overflow".to_owned())?;
    if forecast >= CAMPAIGN_LIMIT_NS {
        return Err(format!(
            "readiness forecast {forecast}ns does not leave sub-60s reserve"
        ));
    }
    let path = readiness_path();
    let schedule_sha256 = sha256_bytes(schedule_json.as_bytes())?;
    let master_sha256 = sha256_file(&root.join("master.json"))?;
    let json = format!(
        concat!(
            "{{\"schema\":\"{}\",\"status\":\"PASS\",",
            "\"measured_rows_started\":false,\"run_directory_exists\":false,",
            "\"expected_rows\":47,\"edit_suboperations\":51,\"transitions\":34,",
            "\"source_tree_blake3\":\"{}\",\"source_manifest_sha256\":\"{}\",",
            "\"executable_path\":\"{}\",\"executable_sha256\":\"{}\",",
            "\"executable_blake3\":\"{}\",\"fixture_master_sha256\":\"{}\",",
            "\"fixture_blake3\":\"{}\",\"schedule_sha256\":\"{}\",",
            "\"store_id\":\"{}\",\"profile\":\"{}\",",
            "\"apfs_identity\":\"{}\",\"reset_wall_ns\":{},",
            "\"reset_limit_ns\":{},\"forecast_non_reset_wall_ns\":{},",
            "\"forecast_campaign_wall_ns\":{},\"forecast_reserve_ns\":{},",
            "\"hard_limit_ns\":{},\"git_commit\":\"{}\",\"dirty_tree\":{}}}\n"
        ),
        READINESS_SCHEMA,
        source.tree_blake3,
        source.manifest_sha256,
        json_escape(&source.executable_path.display().to_string()),
        source.executable_sha256,
        source.executable_blake3,
        master_sha256,
        master.fixture_blake3,
        schedule_sha256,
        master.store_id,
        json_escape(&master.profile),
        json_escape(&master.apfs_identity),
        reset_wall_ns,
        RESET_LIMIT_NS,
        FROZEN_NON_RESET_FORECAST_NS,
        forecast,
        CAMPAIGN_LIMIT_NS - forecast,
        CAMPAIGN_LIMIT_NS,
        source.git_commit,
        source.dirty_tree,
    );
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(io_error)?;
    }
    if path.exists() {
        let preserved = path.with_file_name(format!(
            "layerfs-stage1-apple-edge-readiness-preserved-{}.json",
            unix_ns()?
        ));
        fs::rename(&path, &preserved).map_err(io_error)?;
    }
    durable_write(&path, &json)?;
    println!(
        "stage1.1-readiness status=PASS receipt={} reset_wall_ns={} forecast_campaign_wall_ns={} measured_rows_started=false",
        path.display(), reset_wall_ns, forecast
    );
    Ok(())
}
