use super::artifact::{
    display_error, durable_write, io_error, json_bool, json_escape, json_string, json_u128,
    json_u128_array,
};
use super::environment::{base, environment};
use super::model::{
    Environment, Readiness, StoreSqliteProfile, CAMPAIGN_LIMIT_NS, FORECAST_EDIT_NS,
    FORECAST_MANAGED_NS, FORECAST_MISC_NS, FORECAST_POSTCHECK_ARTIFACT_NS, FORECAST_READ_NS,
    FORECAST_WRITE_NS, READINESS_ARTIFACT_SERIAL, RESET_COUNT, RESET_LIMIT_NS, RESET_RESERVE_NS,
    STORE_CACHE_PAGES, STORE_CACHE_SPILL_PAGES, STORE_PAGE_SIZE,
};
use crate::legacy_full::{Diagnostics, IntegrityMode};
use crate::stage1_fixture::{
    assert_apfs, fixture_root, read_master, regular_file_ceiling_preflight, verify_master,
    verify_user_file_ceiling, workspace_root, Attempt, EvalResult, FILE_BYTES,
};
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::time::{SystemTime, UNIX_EPOCH};
pub(crate) fn readiness_single_file() -> EvalResult<()> {
    let path = workspace_root().join("target/layerfs-stage1-readiness.json");
    let receipt = match readiness() {
        Ok(receipt) => receipt,
        Err(error) => {
            let json = format!(
                "{{\"schema\":\"layerfs-stage1-readiness-failure-v2\",\"status\":\"FAIL\",\"measured_rows_started\":false,\"error\":\"{}\"}}\n",
                json_escape(&error)
            );
            let parent = path
                .parent()
                .ok_or_else(|| "readiness receipt has no parent".to_owned())?;
            return match append_only_readiness_artifact(parent, "failure", &json) {
                Ok(_) => Err(error),
                Err(receipt_error) => Err(format!(
                    "{error}; readiness failure receipt failed: {receipt_error}"
                )),
            };
        }
    };
    if path.exists() {
        let prior = fs::read_to_string(&path).map_err(io_error)?;
        append_only_readiness_artifact(
            path.parent()
                .ok_or_else(|| "readiness receipt has no parent".to_owned())?,
            "preserved",
            &prior,
        )?;
    }
    durable_write(&path, &readiness_json(&receipt))?;
    println!(
        "stage1-readiness status=PASS receipt={} reset_upper_ns={} forecast_campaign_wall_ns={} measured_rows=0",
        path.display(),
        receipt.reset_upper_ns,
        receipt.forecast_campaign_wall_ns
    );
    Ok(())
}
pub(crate) fn append_only_readiness_artifact(
    parent: &Path,
    label: &str,
    contents: &str,
) -> EvalResult<PathBuf> {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(display_error)?
        .as_nanos();
    let path = parent.join(format!(
        "stage1-readiness-{label}-v2-{nonce}-{}-{}.json",
        std::process::id(),
        READINESS_ARTIFACT_SERIAL.fetch_add(1, Ordering::Relaxed)
    ));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .map_err(io_error)?;
    file.write_all(contents.as_bytes()).map_err(io_error)?;
    file.sync_all().map_err(io_error)?;
    File::open(parent)
        .and_then(|directory| directory.sync_all())
        .map_err(io_error)?;
    Ok(path)
}
pub(crate) fn readiness() -> EvalResult<Readiness> {
    if cfg!(debug_assertions) {
        return Err("zero-row readiness requires the release evaluator".to_owned());
    }
    regular_file_ceiling_preflight()?;
    let root = fixture_root();
    verify_user_file_ceiling(&root.join("input"))?;
    let master = read_master(&root)?;
    let apfs = assert_apfs(&root)?;
    let master_digest = verify_master(&root, &master, true)?;
    let expected = base(&master, "read-reconstruct")?;
    let mut reset_observations_ns = Vec::new();
    let mut store_sqlite_profile = None;
    for _ in 0..3 {
        let attempt = Attempt::create("read-reconstruct", expected)?;
        let opened = attempt.open(expected, IntegrityMode::TrustedLocalDev)?;
        let observed =
            validate_store_sqlite_profile(&opened.fs.counter_snapshot().map_err(display_error)?)?;
        if store_sqlite_profile
            .replace(observed)
            .is_some_and(|prior| prior != observed)
        {
            return Err("readiness Store SQLite profile changed across resets".to_owned());
        }
        drop(opened);
        if attempt.clone.wall_ns > RESET_LIMIT_NS {
            return Err(format!(
                "readiness reset {}ns exceeds {RESET_LIMIT_NS}ns",
                attempt.clone.wall_ns
            ));
        }
        reset_observations_ns.push(attempt.clone.wall_ns);
        attempt.cleanup()?;
    }
    let reset_upper_ns = reset_observations_ns
        .iter()
        .copied()
        .max()
        .ok_or_else(|| "readiness produced no reset observations".to_owned())?;
    let forecast_reset_wall_ns = reset_upper_ns
        .checked_mul(u128::from(RESET_COUNT))
        .ok_or_else(|| "reset forecast overflow".to_owned())?;
    let forecast_campaign_wall_ns = forecast_reset_wall_ns
        .checked_add(non_reset_forecast_ns())
        .ok_or_else(|| "campaign forecast overflow".to_owned())?;
    if forecast_reset_wall_ns > RESET_RESERVE_NS {
        return Err(format!(
            "readiness failed: 54-reset reserve {forecast_reset_wall_ns}ns exceeds {RESET_RESERVE_NS}ns"
        ));
    }
    if forecast_campaign_wall_ns > CAMPAIGN_LIMIT_NS {
        return Err(format!(
            "readiness failed: campaign forecast {forecast_campaign_wall_ns}ns exceeds {CAMPAIGN_LIMIT_NS}ns"
        ));
    }
    Ok(Readiness {
        environment: environment()?,
        master_digest,
        reset_observations_ns,
        reset_upper_ns,
        forecast_reset_wall_ns,
        forecast_campaign_wall_ns,
        apfs_identity: apfs.lines().collect::<Vec<_>>().join("\n"),
        store_database_bytes: master
            .bases
            .iter()
            .map(|(name, value)| (name.clone(), value.store_database_bytes))
            .collect(),
        store_sqlite_profile: store_sqlite_profile
            .ok_or_else(|| "readiness observed no Store SQLite profile".to_owned())?,
    })
}
pub(crate) fn validate_store_sqlite_profile(
    diagnostics: &Diagnostics,
) -> EvalResult<StoreSqliteProfile> {
    let observed = StoreSqliteProfile {
        page_size: diagnostics.page_size,
        cache_pages: diagnostics.cache_pages,
        cache_spill_pages: diagnostics.cache_spill_pages,
    };
    let expected = StoreSqliteProfile {
        page_size: STORE_PAGE_SIZE,
        cache_pages: STORE_CACHE_PAGES,
        cache_spill_pages: STORE_CACHE_SPILL_PAGES,
    };
    if observed != expected {
        return Err(format!(
            "readiness Store SQLite profile mismatch: observed {observed:?}, expected {expected:?}"
        ));
    }
    Ok(observed)
}
pub(crate) fn non_reset_forecast_ns() -> u128 {
    FORECAST_READ_NS
        + FORECAST_WRITE_NS
        + FORECAST_EDIT_NS
        + FORECAST_MANAGED_NS
        + FORECAST_MISC_NS
        + FORECAST_POSTCHECK_ARTIFACT_NS
}
pub(crate) fn readiness_json(receipt: &Readiness) -> String {
    let observations = json_u128_array(&receipt.reset_observations_ns);
    let stores = receipt
        .store_database_bytes
        .iter()
        .map(|(name, bytes)| format!("\"{}\":{bytes}", json_escape(name)))
        .collect::<Vec<_>>()
        .join(",");
    format!(
        concat!(
            "{{\"schema\":\"layerfs-stage1-readiness-v2\",\"status\":\"PASS\",",
            "\"measured_rows_started\":false,\"maximum_user_regular_file_bytes\":{},",
            "\"store_database_bytes\":{{{}}},\"fixture_master_blake3\":\"{}\",",
            "\"store_sqlite_profile\":{{\"page_size\":{},\"cache_pages\":{},",
            "\"cache_spill_pages\":{}}},",
            "\"clone_evidence\":\"APFSCloneReturnPlusSealedMasterCustodyNotPerResetFullRehash\",",
            "\"reset_observations_ns\":{},\"reset_upper_ns\":{},",
            "\"preferred_reset_status\":\"{}\",",
            "\"forecast_components_ns\":{{\"reads\":{},\"writes\":{},\"edits\":{},",
            "\"managed\":{},\"misc\":{},\"postchecks_cleanup_artifacts\":{}}},",
            "\"forecast_reset_wall_ns\":{},\"forecast_campaign_wall_ns\":{},",
            "\"fixed_resets\":{},\"git_commit\":\"{}\",\"dirty_tree_blake3\":\"{}\",",
            "\"source_tree_blake3\":\"{}\",\"executable_blake3\":\"{}\",",
            "\"apfs_identity\":\"{}\"}}\n"
        ),
        FILE_BYTES,
        stores,
        receipt.master_digest,
        receipt.store_sqlite_profile.page_size,
        receipt.store_sqlite_profile.cache_pages,
        receipt.store_sqlite_profile.cache_spill_pages,
        observations,
        receipt.reset_upper_ns,
        if receipt.reset_upper_ns <= 2_000_000_000 {
            "PASS"
        } else {
            "REVISE"
        },
        FORECAST_READ_NS,
        FORECAST_WRITE_NS,
        FORECAST_EDIT_NS,
        FORECAST_MANAGED_NS,
        FORECAST_MISC_NS,
        FORECAST_POSTCHECK_ARTIFACT_NS,
        receipt.forecast_reset_wall_ns,
        receipt.forecast_campaign_wall_ns,
        RESET_COUNT,
        json_escape(&receipt.environment.git_commit),
        receipt.environment.dirty_tree_blake3,
        receipt.environment.source_tree_blake3,
        receipt.environment.executable_blake3,
        json_escape(&receipt.apfs_identity),
    )
}
pub(crate) fn admit_readiness(
    environment: &Environment,
    master_digest: &str,
    apfs_identity: &str,
) -> EvalResult<()> {
    let expected_page_size = u128::try_from(STORE_PAGE_SIZE).map_err(display_error)?;
    let expected_cache_pages = u128::try_from(STORE_CACHE_PAGES).map_err(display_error)?;
    let expected_cache_spill_pages =
        u128::try_from(STORE_CACHE_SPILL_PAGES).map_err(display_error)?;
    let path = workspace_root().join("target/layerfs-stage1-readiness.json");
    let json = fs::read_to_string(&path).map_err(|error| {
        format!(
            "zero-row readiness receipt {} is unavailable: {error}",
            path.display()
        )
    })?;
    if json_string(&json, "status")? != "PASS"
        || json_bool(&json, "measured_rows_started")?
        || json_u128(&json, "fixed_resets")? != u128::from(RESET_COUNT)
        || json_u128(&json, "reset_upper_ns")? > RESET_LIMIT_NS
        || json_u128(&json, "forecast_reset_wall_ns")? > RESET_RESERVE_NS
        || json_u128(&json, "forecast_campaign_wall_ns")? > CAMPAIGN_LIMIT_NS
        || json_u128(&json, "page_size")? != expected_page_size
        || json_u128(&json, "cache_pages")? != expected_cache_pages
        || json_u128(&json, "cache_spill_pages")? != expected_cache_spill_pages
        || json_string(&json, "clone_evidence")?
            != "APFSCloneReturnPlusSealedMasterCustodyNotPerResetFullRehash"
        || json_string(&json, "fixture_master_blake3")? != master_digest
        || json_string(&json, "apfs_identity")? != json_escape(apfs_identity)
        || json_string(&json, "git_commit")? != environment.git_commit
        || json_string(&json, "dirty_tree_blake3")? != environment.dirty_tree_blake3
        || json_string(&json, "source_tree_blake3")? != environment.source_tree_blake3
        || json_string(&json, "executable_blake3")? != environment.executable_blake3
    {
        return Err("readiness receipt does not bind this exact frozen release source".to_owned());
    }
    Ok(())
}
pub(crate) fn schedule_json(admitted: bool) -> String {
    format!(
        concat!(
            "{{\"schema\":\"layerfs-stage1-single-file-schedule-v2\",",
            "\"readiness_admitted\":{},\"fixed_resets\":{},\"hard_stop_ns\":{},",
            "\"reset_equation\":\"3+3+3+3+30+3+3+1+1+3+1=54\",",
            "\"operations\":[",
            "{{\"id\":\"A01\",\"resets\":3,\"observations_per_reset\":1}},",
            "{{\"id\":\"A02\",\"resets\":3,\"observations_per_reset\":100}},",
            "{{\"id\":\"A03a\",\"resets\":3}},{{\"id\":\"A03b\",\"resets\":3}},",
            "{{\"id\":\"A04-A08\",\"resets\":30,\"arms\":2,\"samples_per_arm\":3}},",
            "{{\"id\":\"A09\",\"resets\":3}},{{\"id\":\"A10-A12\",\"resets\":3}},",
            "{{\"id\":\"A13\",\"resets\":1,\"observations_per_reset\":11}},",
            "{{\"id\":\"A14\",\"resets\":1,\"revisions\":4}},",
            "{{\"id\":\"A15\",\"resets\":3}},{{\"id\":\"A16\",\"resets\":0}},",
            "{{\"id\":\"A17\",\"resets\":1,\"checkpoints\":100}}]}}\n"
        ),
        admitted, RESET_COUNT, CAMPAIGN_LIMIT_NS
    )
}
