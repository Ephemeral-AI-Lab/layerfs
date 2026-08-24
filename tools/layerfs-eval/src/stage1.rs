use crate::stage1_fixture::{
    assert_apfs, edit_bytes, expected_bytes, fixture_root, hash_file, input_path, read_master,
    regular_file_ceiling_preflight, stream_expected, verify_master, verify_user_file_ceiling,
    workspace_root, Attempt, BaseManifest, CloneReceipt, EvalResult, Master, BUFFER_BYTES,
    FILE_BYTES, FILE_PATH, RANDOM_RANGE_BYTES,
};
use layerfs_sdk::{
    Diagnostics, IntegrityMode, LayerFs, NativeRoute, OpenedLayerFs, OperationDiagnostics,
    RefState, RootId,
};
use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

const RESET_COUNT: u64 = 54;
const RESET_RESERVE_NS: u128 = 15_000_000_000;
const RESET_LIMIT_NS: u128 = 5_000_000_000;
const CAMPAIGN_LIMIT_NS: u128 = 120_000_000_000;
const MIB: u64 = 1_048_576;
static READINESS_ARTIFACT_SERIAL: AtomicU64 = AtomicU64::new(0);

// Frozen poc/14 upper estimates, excluding clone resets. These are forecast
// inputs, never measured results.
const FORECAST_READ_NS: u128 = 7_000_000_000;
const FORECAST_WRITE_NS: u128 = 8_000_000_000;
const FORECAST_EDIT_NS: u128 = 18_000_000_000;
const FORECAST_MANAGED_NS: u128 = 10_000_000_000;
const FORECAST_MISC_NS: u128 = 9_000_000_000;
const FORECAST_POSTCHECK_ARTIFACT_NS: u128 = 8_000_000_000;
const STORE_PAGE_SIZE: i64 = 4_096;
const STORE_CACHE_PAGES: i64 = 1_280;
const STORE_CACHE_SPILL_PAGES: i64 = 1_280;

#[derive(Clone, Debug)]
struct Environment {
    git_commit: String,
    dirty_tree_blake3: String,
    source_tree_blake3: String,
    source_file_count: u64,
    source_files: Vec<String>,
    cargo_lock_blake3: String,
    executable_blake3: String,
    build_profile: &'static str,
    debug_assertions: bool,
    uname: String,
    macos: String,
    apfs_identity: String,
}

#[derive(Clone, Debug)]
struct Readiness {
    environment: Environment,
    master_digest: String,
    reset_observations_ns: Vec<u128>,
    reset_upper_ns: u128,
    forecast_reset_wall_ns: u128,
    forecast_campaign_wall_ns: u128,
    apfs_identity: String,
    store_database_bytes: BTreeMap<String, u64>,
    store_sqlite_profile: StoreSqliteProfile,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct StoreSqliteProfile {
    page_size: i64,
    cache_pages: i64,
    cache_spill_pages: i64,
}

#[derive(Clone, Copy, Debug, Default)]
struct EngineDelta {
    transactions_started: u64,
    transactions_committed: u64,
    transactions_rolled_back: u64,
    statements: u64,
    objects_validated: u64,
    objects_created: u64,
    objects_reused: u64,
    object_bytes_read: u64,
    object_bytes_written: u64,
    range_bytes_requested: u64,
    range_bytes_returned: u64,
    root_verifications: u64,
    root_verification_objects: u64,
    root_verification_bytes: u64,
    fetched_rows: u64,
    fetched_row_authentication_passes: u64,
    fetched_row_role_decode_passes: u64,
    new_object_authentication_passes: u64,
    incumbent_authentication_passes: u64,
    payload_batch_queries: u64,
    payload_batch_references: u64,
    payload_batch_session_maximum: u64,
    put_lookup_statements: u64,
    put_insert_statements: u64,
    created_rows: u64,
    reused_rows: u64,
    publication_commits: u64,
    publication_closure_passes: u64,
    namespace_graph_verification_passes: u64,
    scratch_tables: u64,
    scratch_statements: u64,
    scratch_rows: u64,
    scratch_session_high_water_bytes: u64,
}

#[derive(Default)]
struct DigestSink {
    hasher: blake3::Hasher,
    bytes: u64,
}

impl Write for DigestSink {
    fn write(&mut self, input: &[u8]) -> std::io::Result<usize> {
        if input.len() > BUFFER_BYTES {
            return Err(std::io::Error::other(
                "product emitted a stream buffer larger than 1 MiB",
            ));
        }
        self.hasher.update(input);
        self.bytes = self
            .bytes
            .checked_add(input.len() as u64)
            .ok_or_else(|| std::io::Error::other("digest byte counter overflow"))?;
        Ok(input.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl DigestSink {
    fn finish(self) -> (u64, String) {
        (self.bytes, self.hasher.finalize().to_hex().to_string())
    }
}

struct BoundedRead<R>(R);

impl<R: Read> Read for BoundedRead<R> {
    fn read(&mut self, output: &mut [u8]) -> std::io::Result<usize> {
        if output.len() > BUFFER_BYTES {
            return Err(std::io::Error::other(
                "product requested a stream buffer larger than 1 MiB",
            ));
        }
        self.0.read(output)
    }
}

#[derive(Clone, Debug)]
struct EditCase {
    id: &'static str,
    base: &'static str,
    base_len: u64,
    start: u64,
    delete_len: u64,
    replacement: Vec<u8>,
}

#[derive(Default)]
struct CampaignData {
    reset_count: u64,
    reset_wall_ns: u128,
    open_wall_ns: u128,
    managed_prepare_wall_ns: u128,
    operation_wall_ns: u128,
    postcheck_wall_ns: u128,
    cleanup_wall_ns: u128,
    artifact_wall_ns: u128,
    metrics: BTreeMap<String, Vec<u128>>,
    bytes_per_observation: BTreeMap<String, u64>,
    output_roots: BTreeMap<String, String>,
    last_q_terminal_bytes: Option<u64>,
    store_database_bytes_max: Option<u64>,
    process_resources: Vec<ProcessResources>,
}

#[derive(Clone, Debug)]
struct ProcessResources {
    operation: String,
    observed: bool,
    current_rss_bytes: u64,
    process_peak_rss_bytes: u64,
}

struct Campaign<'a> {
    run: &'a Path,
    started: Instant,
    rows: File,
    data: &'a mut CampaignData,
}

pub fn readiness_single_file() -> EvalResult<()> {
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

fn append_only_readiness_artifact(
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

pub fn run_single_file(run: &Path) -> EvalResult<()> {
    if run.exists() {
        return Err(format!(
            "refusing to overwrite run artifact root {}",
            run.display()
        ));
    }
    if cfg!(debug_assertions) {
        return Err("the frozen campaign requires the release evaluator".to_owned());
    }
    fs::create_dir(run).map_err(io_error)?;
    let started = Instant::now();
    durable_write(
        &run.join("stderr.txt"),
        "campaign did not reach its terminal receipt boundary\n",
    )?;
    durable_write(&run.join("summary.json"), &incomplete_summary_json(0))?;
    durable_write(&run.join("campaign-time.txt"), "0\n")?;
    durable_write(&run.join("rows.jsonl"), "")?;
    arm_hard_stop();
    let mut campaign_data = CampaignData::default();
    let mut start_master_receipt = "Unavailable".to_owned();
    let mut final_master_receipt = "Unavailable".to_owned();
    let mut terminal_receipt = None;
    let mut fd_baseline = None;

    let result = (|| {
        let environment = environment()?;
        durable_write(
            &run.join("environment.json"),
            &environment_json(&environment),
        )?;
        durable_write(&run.join("schedule.json"), &schedule_json(false))?;
        regular_file_ceiling_preflight()?;
        verify_user_file_ceiling(&fixture_root().join("input"))?;
        let master = read_master(&fixture_root())?;
        let start_master_digest = verify_master(&fixture_root(), &master, true)?;
        start_master_receipt.clone_from(&start_master_digest);
        let apfs_identity = assert_apfs(&fixture_root())?;
        admit_readiness(&environment, &start_master_digest, &apfs_identity)?;
        fs::copy(fixture_root().join("master.json"), run.join("master.json")).map_err(io_error)?;
        durable_write(&run.join("schedule.json"), &schedule_json(true))?;
        let rows = OpenOptions::new()
            .append(true)
            .open(run.join("rows.jsonl"))
            .map_err(io_error)?;
        let mut campaign = Campaign {
            run,
            started,
            rows,
            data: &mut campaign_data,
        };
        campaign.observe_process_resources("campaign-baseline")?;
        let observed_fd_baseline = fd_count()?;
        fd_baseline = Some(observed_fd_baseline);
        execute_campaign(&mut campaign, &master)?;
        campaign.check_deadline()?;
        if campaign.data.reset_count != RESET_COUNT {
            return Err(format!(
                "reset cardinality mismatch: {} != {RESET_COUNT}",
                campaign.data.reset_count
            ));
        }
        let final_master_digest = verify_master(&fixture_root(), &master, true)?;
        final_master_receipt.clone_from(&final_master_digest);
        if final_master_digest != start_master_digest {
            return Err("sealed master changed during campaign".to_owned());
        }
        let terminal = terminal_resources(observed_fd_baseline)?;
        terminal_receipt = Some(terminal.clone());
        let terminal_error = append_a16(&mut campaign, &terminal, true)?;
        let artifact_started = Instant::now();
        campaign.rows.sync_all().map_err(io_error)?;
        if let Some(error) = terminal_error {
            return Err(error);
        }
        durable_write(
            &run.join("summary.md"),
            "# LayerFS Stage One Part 1\n\nFAIL: campaign did not reach its terminal receipt boundary.\n",
        )?;
        let provisional_wall = campaign.started.elapsed().as_nanos();
        durable_write(
            &run.join("summary.json"),
            &incomplete_summary_json(provisional_wall),
        )?;
        durable_write(
            &run.join("campaign-time.txt"),
            &format!("{provisional_wall}\n"),
        )?;
        campaign.data.artifact_wall_ns = campaign
            .data
            .artifact_wall_ns
            .checked_add(artifact_started.elapsed().as_nanos())
            .ok_or_else(|| "artifact timer overflow".to_owned())?;
        let wall = campaign.started.elapsed().as_nanos();
        let summary = summary_json(
            "PASS",
            None,
            campaign.data,
            wall,
            &start_master_digest,
            &final_master_digest,
            &terminal,
        )?;
        if wall > CAMPAIGN_LIMIT_NS {
            return Err(format!(
                "hard campaign stop exceeded: {wall}ns > {CAMPAIGN_LIMIT_NS}ns"
            ));
        }
        // The provisional durable summary/time writes are inside `wall`. These
        // final receipt rewrites publish that endpoint and are necessarily
        // outside the timestamp they report.
        durable_write(
            &run.join("summary.md"),
            &summary_markdown(campaign.data, wall)?,
        )?;
        durable_write(&run.join("campaign-time.txt"), &format!("{wall}\n"))?;
        fs::remove_file(run.join("stderr.txt")).map_err(io_error)?;
        File::open(run)
            .and_then(|directory| directory.sync_all())
            .map_err(io_error)?;
        durable_replace(&run.join("summary.json"), &summary)?;
        Ok(())
    })();

    if let Err(error) = &result {
        let artifact_started = Instant::now();
        let a16_error = if terminal_receipt.is_none() {
            match append_failure_a16(run, started, &mut campaign_data, fd_baseline) {
                Ok(terminal) => {
                    terminal_receipt = Some(terminal);
                    None
                }
                Err(error) => Some(error),
            }
        } else {
            None
        };
        let rows_sync = sync_rows(run);
        let mut diagnostic = error.clone();
        if let Some(a16) = a16_error {
            diagnostic.push_str(&format!("; A16 append failed: {a16}"));
        }
        if let Err(sync) = &rows_sync {
            diagnostic.push_str(&format!("; rows sync failed: {sync}"));
        }
        let provisional_wall = started.elapsed().as_nanos();
        let _ = durable_write(
            &run.join("campaign-time.txt"),
            &format!("{provisional_wall}\n"),
        );
        let _ = durable_write(&run.join("stderr.txt"), &format!("{diagnostic}\n"));
        let _ = durable_write(
            &run.join("summary.md"),
            &format!("# LayerFS Stage One Part 1\n\nFAIL: {diagnostic}\n"),
        );
        campaign_data.artifact_wall_ns = campaign_data
            .artifact_wall_ns
            .saturating_add(artifact_started.elapsed().as_nanos());
        let wall = started.elapsed().as_nanos();
        let terminal = terminal_receipt.as_ref().cloned().unwrap_or_default();
        if let Ok(summary) = summary_json(
            "FAIL",
            Some(&diagnostic),
            &campaign_data,
            wall,
            &start_master_receipt,
            &final_master_receipt,
            &terminal,
        ) {
            let _ = durable_write(&run.join("campaign-time.txt"), &format!("{wall}\n"));
            let _ = durable_replace(&run.join("summary.json"), &summary);
        }
    }
    cancel_hard_stop();
    result
}

fn incomplete_summary_json(wall: u128) -> String {
    format!(
        "{{\"schema\":\"layerfs-stage1-summary-v2\",\"status\":\"FAIL\",\"campaign_complete_wall_ns\":{wall},\"campaign_equation\":{{\"reset_ns\":0,\"open_ns\":0,\"managed_prepare_ns\":0,\"operation_ns\":0,\"postcheck_ns\":0,\"cleanup_ns\":0,\"artifact_ns\":0,\"timer_residual_ns\":{wall},\"sum_ns\":{wall},\"closed\":true}},\"resets\":0,\"statistics\":{{}},\"targets\":{{}},\"process_resources\":{{\"observations\":[],\"first_64_mib_crossing\":null}},\"error\":\"campaign did not reach its terminal receipt boundary\"}}\n"
    )
}

#[cfg(target_os = "macos")]
fn arm_hard_stop() {
    unsafe extern "C" {
        fn alarm(seconds: u32) -> u32;
    }
    // SAFETY: process-global SIGALRM retains its default terminating action;
    // this campaign owns the evaluator process and has no other alarm user.
    unsafe {
        alarm(120);
    }
}

#[cfg(not(target_os = "macos"))]
fn arm_hard_stop() {}

#[cfg(target_os = "macos")]
fn cancel_hard_stop() {
    unsafe extern "C" {
        fn alarm(seconds: u32) -> u32;
    }
    // SAFETY: cancelling the evaluator-owned alarm has no pointer/state input.
    unsafe {
        alarm(0);
    }
}

#[cfg(not(target_os = "macos"))]
fn cancel_hard_stop() {}

fn readiness() -> EvalResult<Readiness> {
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

fn validate_store_sqlite_profile(diagnostics: &Diagnostics) -> EvalResult<StoreSqliteProfile> {
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

fn non_reset_forecast_ns() -> u128 {
    FORECAST_READ_NS
        + FORECAST_WRITE_NS
        + FORECAST_EDIT_NS
        + FORECAST_MANAGED_NS
        + FORECAST_MISC_NS
        + FORECAST_POSTCHECK_ARTIFACT_NS
}

fn readiness_json(receipt: &Readiness) -> String {
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

fn admit_readiness(
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

fn schedule_json(admitted: bool) -> String {
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

fn execute_campaign(campaign: &mut Campaign<'_>, master: &Master) -> EvalResult<()> {
    run_a01(campaign, master)?;
    run_a02(campaign, master)?;
    run_stream_write(campaign, master, "A03a", "import-genesis", false)?;
    run_stream_write(campaign, master, "A03b", "replace-existing", true)?;
    run_edit_matrix(campaign, master)?;
    run_a09(campaign, master)?;
    run_a10_a12(campaign, master)?;
    run_a13(campaign, master)?;
    run_a14(campaign, master)?;
    run_a15(campaign, master)?;
    run_a17(campaign, master)?;
    Ok(())
}

impl Campaign<'_> {
    fn check_deadline(&self) -> EvalResult<()> {
        if self.started.elapsed().as_nanos() >= CAMPAIGN_LIMIT_NS {
            Err("hard campaign deadline reached".to_owned())
        } else {
            Ok(())
        }
    }

    fn attempt(&mut self, name: &str, expected: &BaseManifest) -> EvalResult<Attempt> {
        self.check_deadline()?;
        let attempt = Attempt::create(name, expected)?;
        if attempt.clone.wall_ns > RESET_LIMIT_NS {
            return Err(format!(
                "measured reset {}ns exceeds {RESET_LIMIT_NS}ns",
                attempt.clone.wall_ns
            ));
        }
        self.data.reset_count = self
            .data
            .reset_count
            .checked_add(1)
            .ok_or_else(|| "reset counter overflow".to_owned())?;
        self.data.reset_wall_ns = self
            .data
            .reset_wall_ns
            .checked_add(attempt.clone.wall_ns)
            .ok_or_else(|| "reset timer overflow".to_owned())?;
        Ok(attempt)
    }

    fn open(
        &mut self,
        attempt: &Attempt,
        expected: &BaseManifest,
    ) -> EvalResult<(OpenedLayerFs, u128)> {
        self.check_deadline()?;
        let started = Instant::now();
        let opened = attempt.open(expected, IntegrityMode::TrustedLocalDev)?;
        let wall = started.elapsed().as_nanos();
        self.data.open_wall_ns = self
            .data
            .open_wall_ns
            .checked_add(wall)
            .ok_or_else(|| "open timer overflow".to_owned())?;
        Ok((opened, wall))
    }

    fn cleanup(&mut self, attempt: Attempt) -> EvalResult<u128> {
        let started = Instant::now();
        attempt.cleanup()?;
        let wall = started.elapsed().as_nanos();
        self.data.cleanup_wall_ns = self
            .data
            .cleanup_wall_ns
            .checked_add(wall)
            .ok_or_else(|| "cleanup timer overflow".to_owned())?;
        Ok(wall)
    }

    fn operation_wall(&mut self, wall: u128) -> EvalResult<()> {
        self.data.operation_wall_ns = self
            .data
            .operation_wall_ns
            .checked_add(wall)
            .ok_or_else(|| "operation timer overflow".to_owned())?;
        Ok(())
    }

    fn postcheck_wall(&mut self, wall: u128) -> EvalResult<()> {
        self.data.postcheck_wall_ns = self
            .data
            .postcheck_wall_ns
            .checked_add(wall)
            .ok_or_else(|| "postcheck timer overflow".to_owned())?;
        Ok(())
    }

    fn metric(&mut self, name: &str, wall: u128, bytes: Option<u64>) -> EvalResult<()> {
        self.data
            .metrics
            .entry(name.to_owned())
            .or_default()
            .push(wall);
        if let Some(bytes) = bytes {
            match self
                .data
                .bytes_per_observation
                .insert(name.to_owned(), bytes)
            {
                Some(previous) if previous != bytes => {
                    return Err(format!("metric {name} byte population changed"));
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn bind_output_root(&mut self, name: &str, root: RootId) -> EvalResult<()> {
        let root = root.to_string();
        match self.data.output_roots.insert(name.to_owned(), root.clone()) {
            Some(previous) if previous != root => {
                Err(format!("{name} output root changed across cloned bases"))
            }
            _ => Ok(()),
        }
    }

    fn observe_process_resources(&mut self, operation: &str) -> EvalResult<ProcessResources> {
        let resources = process_resources(operation)?;
        self.data.process_resources.push(resources.clone());
        Ok(resources)
    }

    fn row(&mut self, row: String) -> EvalResult<()> {
        let mut operation = json_string(&row, "id")?;
        if let Ok(arm) = json_string(&row, "arm") {
            operation.push('/');
            operation.push_str(&arm);
        }
        if let Ok(sample) = json_u128(&row, "sample") {
            operation.push_str(&format!("/sample-{sample}"));
        }
        if let Ok(position) = json_string(&row, "position") {
            operation.push('/');
            operation.push_str(&position);
        }
        let resources = process_resources(&operation)?;
        self.row_with_resources(row, resources)
    }

    fn row_with_resources(&mut self, row: String, resources: ProcessResources) -> EvalResult<()> {
        let started = Instant::now();
        if !self.run.is_dir() {
            return Err("campaign artifact root disappeared".to_owned());
        }
        let row = row
            .strip_suffix('}')
            .map(|body| {
                format!(
                    "{body},\"process_resources\":{}}}",
                    process_resources_json(&resources)
                )
            })
            .ok_or_else(|| "row JSON must be an object".to_owned())?;
        let row = row
            .strip_prefix('{')
            .map(|body| format!("{{\"schema\":\"layerfs-stage1-row-v1\",{body}"))
            .ok_or_else(|| "row JSON must be an object".to_owned())?;
        self.rows.write_all(row.as_bytes()).map_err(io_error)?;
        self.rows.write_all(b"\n").map_err(io_error)?;
        self.rows.flush().map_err(io_error)?;
        self.data.artifact_wall_ns = self
            .data
            .artifact_wall_ns
            .checked_add(started.elapsed().as_nanos())
            .ok_or_else(|| "artifact timer overflow".to_owned())?;
        self.data.process_resources.push(resources);
        Ok(())
    }

    fn store_database(&mut self, bytes: Option<u64>) {
        if let Some(bytes) = bytes {
            self.data.store_database_bytes_max = Some(
                self.data
                    .store_database_bytes_max
                    .map_or(bytes, |current| current.max(bytes)),
            );
        }
    }
}

fn run_a01(campaign: &mut Campaign<'_>, master: &Master) -> EvalResult<()> {
    let expected = base(master, "read-reconstruct")?;
    for sample in 1..=3 {
        let complete_started = Instant::now();
        let attempt = campaign.attempt("read-reconstruct", expected)?;
        let clone = attempt.clone.clone();
        let (opened, open_wall) = campaign.open(&attempt, expected)?;
        let before = opened.fs.counter_snapshot().map_err(display_error)?;
        let mut sink = DigestSink::default();
        let mut counters = OperationDiagnostics::default();
        let operation_started = Instant::now();
        for index in 0..100_u64 {
            counters = merge_counters(
                counters,
                opened
                    .fs
                    .read_range(
                        expected.root,
                        FILE_PATH,
                        index * MIB..(index + 1) * MIB,
                        &mut sink,
                    )
                    .map_err(display_error)?,
            )?;
        }
        let operation_wall = operation_started.elapsed().as_nanos();
        campaign.operation_wall(operation_wall)?;
        let after = opened.fs.diagnostics().map_err(display_error)?;
        campaign.store_database(after.database_bytes);
        let engine = engine_delta(&before, &after)?;
        verify_read_only_engine(&engine)?;
        verify_direct_read(&counters)?;
        let post_started = Instant::now();
        let (bytes, digest) = sink.finish();
        if bytes != FILE_BYTES || digest != master.raw_digest {
            return Err(format!("A01 sample {sample} canonical digest mismatch"));
        }
        let post_wall = post_started.elapsed().as_nanos();
        campaign.postcheck_wall(post_wall)?;
        campaign.metric("A01", operation_wall, Some(FILE_BYTES))?;
        drop(opened);
        let cleanup_wall = campaign.cleanup(attempt)?;
        campaign.row(format!(
            "{{\"id\":\"A01\",\"sample\":{sample},\"cache\":\"same-open-warm-or-unknown\",\"timing\":{{\"reset_ns\":{},\"open_ns\":{open_wall},\"operation_wall_ns\":{operation_wall},\"attributed_wall_ns\":{operation_wall},\"unattributed_wall_ns\":0,\"postcheck_ns\":{post_wall},\"cleanup_ns\":{cleanup_wall},\"complete_sample_wall_ns\":{}}},\"oracle\":{{\"bytes\":{bytes},\"blake3\":\"{digest}\",\"native_workspace\":false}},\"clone\":{},\"operation_counters\":{},\"engine_delta\":{}}}",
            clone.wall_ns,
            complete_started.elapsed().as_nanos(),
            clone_json(&clone),
            counters_json(&counters),
            engine_json(&engine),
        ))?;
    }
    Ok(())
}

fn run_a02(campaign: &mut Campaign<'_>, master: &Master) -> EvalResult<()> {
    let expected = base(master, "read-reconstruct")?;
    let blocks = FILE_BYTES / RANDOM_RANGE_BYTES;
    let mut seen = std::collections::BTreeSet::new();
    for batch in 0..3_u64 {
        let complete_started = Instant::now();
        let attempt = campaign.attempt("read-reconstruct", expected)?;
        let clone = attempt.clone.clone();
        let (opened, open_wall) = campaign.open(&attempt, expected)?;
        let before = opened.fs.counter_snapshot().map_err(display_error)?;
        let mut observations = Vec::with_capacity(100);
        let mut offsets = Vec::with_capacity(100);
        let mut counters = OperationDiagnostics::default();
        let operation_started = Instant::now();
        for within in 0..100_u64 {
            let global = batch * 100 + within;
            let offset = ((global * 521 + 0x51) % blocks) * RANDOM_RANGE_BYTES;
            if !seen.insert(offset) {
                return Err(format!(
                    "A02 deterministic permutation repeated offset {offset}"
                ));
            }
            let mut output = Vec::with_capacity(RANDOM_RANGE_BYTES as usize);
            let call_started = Instant::now();
            let observed = opened
                .fs
                .read_range(
                    expected.root,
                    FILE_PATH,
                    offset..offset + RANDOM_RANGE_BYTES,
                    &mut output,
                )
                .map_err(display_error)?;
            let call_wall = call_started.elapsed().as_nanos();
            if output != expected_bytes(offset, RANDOM_RANGE_BYTES as usize)? {
                return Err(format!("A02 range oracle mismatch at {offset}"));
            }
            counters = merge_counters(counters, observed)?;
            observations.push(call_wall);
            offsets.push(offset);
            campaign.metric("A02", call_wall, Some(RANDOM_RANGE_BYTES))?;
        }
        let operation_wall = operation_started.elapsed().as_nanos();
        campaign.operation_wall(operation_wall)?;
        let after = opened.fs.diagnostics().map_err(display_error)?;
        campaign.store_database(after.database_bytes);
        let engine = engine_delta(&before, &after)?;
        verify_read_only_engine(&engine)?;
        verify_direct_read(&counters)?;
        drop(opened);
        let cleanup_wall = campaign.cleanup(attempt)?;
        campaign.row(format!(
            "{{\"id\":\"A02\",\"batch\":{},\"cache\":\"same-open-warm-or-unknown\",\"timing\":{{\"reset_ns\":{},\"open_ns\":{open_wall},\"operation_wall_ns\":{operation_wall},\"attributed_wall_ns\":{},\"unattributed_wall_ns\":{},\"cleanup_ns\":{cleanup_wall},\"complete_sample_wall_ns\":{}}},\"raw_observations_ns\":{},\"range_offsets\":{},\"oracle\":{{\"ranges\":100,\"bytes_per_range\":{RANDOM_RANGE_BYTES}}},\"clone\":{},\"operation_counters\":{},\"engine_delta\":{}}}",
            batch + 1,
            clone.wall_ns,
            observations.iter().sum::<u128>(),
            timer_residual(operation_wall, observations.iter().sum::<u128>())?,
            complete_started.elapsed().as_nanos(),
            json_u128_array(&observations),
            json_u64_array(&offsets),
            clone_json(&clone),
            counters_json(&counters),
            engine_json(&engine),
        ))?;
    }
    if seen.len() != 300 {
        return Err("A02 must contain 300 globally non-overlapping ranges".to_owned());
    }
    Ok(())
}

fn run_stream_write(
    campaign: &mut Campaign<'_>,
    master: &Master,
    id: &str,
    base_name: &str,
    replacement: bool,
) -> EvalResult<()> {
    let expected = base(master, base_name)?;
    let wanted_digest = if replacement {
        &master.replacement_digest
    } else {
        &master.raw_digest
    };
    for sample in 1..=3 {
        let complete_started = Instant::now();
        let attempt = campaign.attempt(base_name, expected)?;
        let clone = attempt.clone.clone();
        let (opened, open_wall) = campaign.open(&attempt, expected)?;
        let before = opened.fs.counter_snapshot().map_err(display_error)?;
        let input = BoundedRead(File::open(input_path(replacement)).map_err(io_error)?);
        let operation_started = Instant::now();
        let (state, counters) = opened
            .fs
            .replace_file_observed(&expected_ref(expected), FILE_PATH, input)
            .map_err(display_error)?;
        let operation_wall = operation_started.elapsed().as_nanos();
        campaign.operation_wall(operation_wall)?;
        let after = opened.fs.diagnostics().map_err(display_error)?;
        campaign.store_database(after.database_bytes);
        let engine = engine_delta(&before, &after)?;
        verify_state_change(&engine, 1)?;
        verify_operation_resources(&counters)?;
        campaign.bind_output_root(id, state.root)?;
        let post_started = Instant::now();
        if opened.fs.current_head("main").map_err(display_error)? != state {
            return Err(format!("{id} did not publish its exact RefState"));
        }
        let (bytes, digest, _) = canonical_digest(&opened.fs, state.root)?;
        if bytes != FILE_BYTES || &digest != wanted_digest {
            return Err(format!("{id} sample {sample} output mismatch"));
        }
        let post_wall = post_started.elapsed().as_nanos();
        campaign.postcheck_wall(post_wall)?;
        campaign.metric(id, operation_wall, Some(FILE_BYTES))?;
        campaign.data.last_q_terminal_bytes = Some(after.operation_q_current_bytes);
        drop(opened);
        let cleanup_wall = campaign.cleanup(attempt)?;
        campaign.row(format!(
            "{{\"id\":\"{id}\",\"sample\":{sample},\"cache\":\"cold-destination\",\"timing\":{{\"reset_ns\":{},\"open_ns\":{open_wall},\"operation_wall_ns\":{operation_wall},\"attributed_wall_ns\":{operation_wall},\"unattributed_wall_ns\":0,\"postcheck_ns\":{post_wall},\"cleanup_ns\":{cleanup_wall},\"complete_sample_wall_ns\":{}}},\"oracle\":{{\"bytes\":{bytes},\"blake3\":\"{digest}\",\"accepted_ref\":\"{}\"}},\"store_database_bytes\":{},\"clone\":{},\"operation_counters\":{},\"engine_delta\":{}}}",
            clone.wall_ns,
            complete_started.elapsed().as_nanos(),
            state.root,
            option_u64_json(after.database_bytes),
            clone_json(&clone),
            counters_json(&counters),
            engine_json(&engine),
        ))?;
    }
    Ok(())
}

fn edit_cases() -> Vec<EditCase> {
    let insert_base = FILE_BYTES - 8_192;
    let append_base = FILE_BYTES - 4_096;
    let delete_start = ((FILE_BYTES * 2 / 3) / 4_096) * 4_096;
    vec![
        EditCase {
            id: "A04",
            base: "overwrite",
            base_len: FILE_BYTES,
            start: FILE_BYTES / 2 - 2_048,
            delete_len: 4_096,
            replacement: edit_bytes(0x44, 4_096),
        },
        EditCase {
            id: "A05",
            base: "insert",
            base_len: insert_base,
            start: insert_base / 2 - 4_096,
            delete_len: 0,
            replacement: edit_bytes(0x45, 8_192),
        },
        EditCase {
            id: "A06",
            base: "delete",
            base_len: FILE_BYTES,
            start: delete_start,
            delete_len: 4_096,
            replacement: Vec::new(),
        },
        EditCase {
            id: "A07",
            base: "append",
            base_len: append_base,
            start: append_base,
            delete_len: 0,
            replacement: edit_bytes(0x47, 4_096),
        },
        EditCase {
            id: "A08",
            base: "truncate",
            base_len: FILE_BYTES,
            start: FILE_BYTES - 4_096,
            delete_len: 4_096,
            replacement: Vec::new(),
        },
    ]
}

fn run_edit_matrix(campaign: &mut Campaign<'_>, master: &Master) -> EvalResult<()> {
    for case in edit_cases() {
        for sample in 1..=3 {
            run_logical_edit(campaign, master, &case, sample)?;
            run_native_edit(campaign, master, &case, sample)?;
        }
    }
    Ok(())
}

fn run_logical_edit(
    campaign: &mut Campaign<'_>,
    master: &Master,
    case: &EditCase,
    sample: u64,
) -> EvalResult<()> {
    let expected = base(master, case.base)?;
    let complete_started = Instant::now();
    let attempt = campaign.attempt(case.base, expected)?;
    let clone = attempt.clone.clone();
    let (opened, open_wall) = campaign.open(&attempt, expected)?;
    let before = opened.fs.counter_snapshot().map_err(display_error)?;
    let operation_started = Instant::now();
    let (state, counters) = opened
        .fs
        .replace_range_observed(
            &expected_ref(expected),
            FILE_PATH,
            case.start,
            case.delete_len,
            std::io::Cursor::new(case.replacement.as_slice()),
        )
        .map_err(display_error)?;
    let operation_wall = operation_started.elapsed().as_nanos();
    campaign.operation_wall(operation_wall)?;
    let after = opened.fs.diagnostics().map_err(display_error)?;
    campaign.store_database(after.database_bytes);
    let engine = engine_delta(&before, &after)?;
    verify_state_change(&engine, 1)?;
    verify_logical_locality(&counters, case.replacement.len() as u64)?;
    let result_len = edit_result_len(case)?;
    let expected_digest = splice_digest(case)?;
    let post_started = Instant::now();
    let (bytes, digest, _) = canonical_digest(&opened.fs, state.root)?;
    if bytes != result_len || digest != expected_digest {
        return Err(format!(
            "{} logical sample {sample} output mismatch",
            case.id
        ));
    }
    verify_old_root_range(&opened.fs, expected.root, case)?;
    if opened.fs.current_head("main").map_err(display_error)? != state {
        return Err(format!("{} logical exact RefState mismatch", case.id));
    }
    let post_wall = post_started.elapsed().as_nanos();
    campaign.postcheck_wall(post_wall)?;
    let metric = format!("{}/logical", case.id);
    campaign.metric(&metric, operation_wall, None)?;
    campaign.bind_output_root(&format!("{}/logical", case.id), state.root)?;
    campaign.data.last_q_terminal_bytes = Some(after.operation_q_current_bytes);
    drop(opened);
    let cleanup_wall = campaign.cleanup(attempt)?;
    campaign.row(format!(
        "{{\"id\":\"{}\",\"arm\":\"logical\",\"sample\":{sample},\"cache\":\"same-open-warm-or-unknown\",\"operand\":{},\"timing\":{{\"reset_ns\":{},\"open_ns\":{open_wall},\"operation_wall_ns\":{operation_wall},\"logical_edit_wall_ns\":{operation_wall},\"attributed_wall_ns\":{operation_wall},\"unattributed_wall_ns\":0,\"postcheck_ns\":{post_wall},\"cleanup_ns\":{cleanup_wall},\"complete_sample_wall_ns\":{}}},\"oracle\":{{\"bytes\":{bytes},\"blake3\":\"{digest}\",\"old_root_readable\":true}},\"locality_evidence\":{},\"clone\":{},\"operation_counters\":{},\"engine_delta\":{}}}",
        case.id,
        edit_case_json(case)?,
        clone.wall_ns,
        complete_started.elapsed().as_nanos(),
        locality_evidence_json(),
        clone_json(&clone),
        counters_json(&counters),
        engine_json(&engine),
    ))
}

fn run_native_edit(
    campaign: &mut Campaign<'_>,
    master: &Master,
    case: &EditCase,
    sample: u64,
) -> EvalResult<()> {
    let expected = base(master, case.base)?;
    let complete_started = Instant::now();
    let attempt = campaign.attempt(case.base, expected)?;
    let clone = attempt.clone.clone();
    let (opened, open_wall) = campaign.open(&attempt, expected)?;
    let prepare_started = Instant::now();
    let (mut managed, prepare_counters) = opened
        .fs
        .materialize_managed_observed(expected.root)
        .map_err(display_error)?;
    let prepare_wall = prepare_started.elapsed().as_nanos();
    verify_operation_resources(&prepare_counters)?;
    if prepare_counters.workspace_materializations != 1 {
        return Err(format!("{} native preparation lifecycle mismatch", case.id));
    }
    campaign.data.managed_prepare_wall_ns = campaign
        .data
        .managed_prepare_wall_ns
        .checked_add(prepare_wall)
        .ok_or_else(|| "managed prepare timer overflow".to_owned())?;
    let before = opened.fs.counter_snapshot().map_err(display_error)?;
    let operation_started = Instant::now();
    let edit_started = Instant::now();
    let edit_counters = managed
        .replace_observed(FILE_PATH, case.start, case.delete_len, &case.replacement)
        .map_err(display_error)?;
    let edit_wall = edit_started.elapsed().as_nanos();
    let checkpoint_started = Instant::now();
    let (state, checkpoint_counters) = managed.checkpoint_observed().map_err(display_error)?;
    let checkpoint_wall = checkpoint_started.elapsed().as_nanos();
    let operation_wall = operation_started.elapsed().as_nanos();
    campaign.operation_wall(operation_wall)?;
    let counters = merge_counters(edit_counters, checkpoint_counters)?;
    verify_native_edit_shape(&counters, case)?;
    let after = opened.fs.diagnostics().map_err(display_error)?;
    campaign.store_database(after.database_bytes);
    let engine = engine_delta(&before, &after)?;
    verify_state_change(&engine, 1)?;
    let result_len = edit_result_len(case)?;
    let expected_digest = splice_digest(case)?;
    let post_started = Instant::now();
    let mut sink = DigestSink::default();
    managed
        .read_to(FILE_PATH, &mut sink)
        .map_err(display_error)?;
    let (bytes, digest) = sink.finish();
    if bytes != result_len || digest != expected_digest {
        return Err(format!(
            "{} native sample {sample} output mismatch",
            case.id
        ));
    }
    verify_old_root_range(&opened.fs, expected.root, case)?;
    if opened.fs.current_head("main").map_err(display_error)? != state {
        return Err(format!("{} native exact RefState mismatch", case.id));
    }
    let post_wall = post_started.elapsed().as_nanos();
    campaign.postcheck_wall(post_wall)?;
    let metric = format!("{}/native-edit-plus-checkpoint", case.id);
    campaign.metric(&metric, operation_wall, None)?;
    managed.discard().map_err(display_error)?;
    campaign.data.last_q_terminal_bytes = Some(after.operation_q_current_bytes);
    drop(managed);
    drop(opened);
    let cleanup_wall = campaign.cleanup(attempt)?;
    campaign.row(format!(
        "{{\"id\":\"{}\",\"arm\":\"native\",\"sample\":{sample},\"cache\":\"cold-destination\",\"operand\":{},\"timing\":{{\"reset_ns\":{},\"open_ns\":{open_wall},\"managed_prepare_wall_ns\":{prepare_wall},\"native_edit_wall_ns\":{edit_wall},\"durable_checkpoint_wall_ns\":{checkpoint_wall},\"edit_plus_checkpoint_wall_ns\":{operation_wall},\"operation_wall_ns\":{operation_wall},\"attributed_wall_ns\":{},\"unattributed_wall_ns\":{},\"postcheck_ns\":{post_wall},\"cleanup_ns\":{cleanup_wall},\"complete_sample_wall_ns\":{}}},\"oracle\":{{\"bytes\":{bytes},\"blake3\":\"{digest}\",\"old_root_readable\":true}},\"locality_evidence\":{},\"clone\":{},\"managed_prepare_counters\":{},\"operation_counters\":{},\"engine_delta\":{}}}",
        case.id,
        edit_case_json(case)?,
        clone.wall_ns,
        edit_wall + checkpoint_wall,
        timer_residual(operation_wall, edit_wall + checkpoint_wall)?,
        complete_started.elapsed().as_nanos(),
        locality_evidence_json(),
        clone_json(&clone),
        counters_json(&prepare_counters),
        counters_json(&counters),
        engine_json(&engine),
    ))
}

fn run_a09(campaign: &mut Campaign<'_>, master: &Master) -> EvalResult<()> {
    let expected = base(master, "read-reconstruct")?;
    for sample in 1..=3 {
        let complete_started = Instant::now();
        let attempt = campaign.attempt("read-reconstruct", expected)?;
        let clone = attempt.clone.clone();
        let (opened, open_wall) = campaign.open(&attempt, expected)?;
        let before = opened.fs.counter_snapshot().map_err(display_error)?;
        let mut sink = DigestSink::default();
        let operation_started = Instant::now();
        let counters = opened
            .fs
            .read_to(expected.root, FILE_PATH, &mut sink)
            .map_err(display_error)?;
        verify_operation_resources(&counters)?;
        let operation_wall = operation_started.elapsed().as_nanos();
        campaign.operation_wall(operation_wall)?;
        let after = opened.fs.diagnostics().map_err(display_error)?;
        campaign.store_database(after.database_bytes);
        let engine = engine_delta(&before, &after)?;
        verify_read_only_engine(&engine)?;
        verify_direct_read(&counters)?;
        let post_started = Instant::now();
        let (bytes, digest) = sink.finish();
        if bytes != FILE_BYTES || digest != master.raw_digest {
            return Err(format!("A09 sample {sample} reconstruction mismatch"));
        }
        let post_wall = post_started.elapsed().as_nanos();
        campaign.postcheck_wall(post_wall)?;
        campaign.metric("A09", operation_wall, Some(FILE_BYTES))?;
        drop(opened);
        let cleanup_wall = campaign.cleanup(attempt)?;
        campaign.row(format!(
            "{{\"id\":\"A09\",\"sample\":{sample},\"cache\":\"same-open-warm-or-unknown\",\"timing\":{{\"reset_ns\":{},\"open_ns\":{open_wall},\"operation_wall_ns\":{operation_wall},\"attributed_wall_ns\":{operation_wall},\"unattributed_wall_ns\":0,\"postcheck_ns\":{post_wall},\"cleanup_ns\":{cleanup_wall},\"complete_sample_wall_ns\":{}}},\"oracle\":{{\"bytes\":{bytes},\"blake3\":\"{digest}\",\"native_workspace\":false}},\"clone\":{},\"operation_counters\":{},\"engine_delta\":{}}}",
            clone.wall_ns,
            complete_started.elapsed().as_nanos(),
            clone_json(&clone),
            counters_json(&counters),
            engine_json(&engine),
        ))?;
    }
    Ok(())
}

fn run_a10_a12(campaign: &mut Campaign<'_>, master: &Master) -> EvalResult<()> {
    let expected = base(master, "refresh-a-b")?;
    let root_a = expected
        .root_a
        .ok_or_else(|| "refresh-a-b root A missing".to_owned())?;
    let root_b = expected
        .root_b
        .ok_or_else(|| "refresh-a-b root B missing".to_owned())?;
    if expected.root != root_a {
        return Err("refresh-a-b main must start at retained A".to_owned());
    }
    let refresh_case = EditCase {
        id: "A12",
        base: "refresh-a-b",
        base_len: FILE_BYTES,
        start: FILE_BYTES / 2 - 2_048,
        delete_len: 4_096,
        replacement: edit_bytes(0x42, 4_096),
    };
    let target_digest = splice_digest(&refresh_case)?;

    for sample in 1..=3 {
        let complete_started = Instant::now();
        let attempt = campaign.attempt("refresh-a-b", expected)?;
        let clone = attempt.clone.clone();
        let (opened, open_wall) = campaign.open(&attempt, expected)?;

        let materialize_before = opened.fs.counter_snapshot().map_err(display_error)?;
        let materialize_started = Instant::now();
        let (mut managed, materialize_counters) = opened
            .fs
            .materialize_managed_observed(root_a)
            .map_err(display_error)?;
        let materialize_wall = materialize_started.elapsed().as_nanos();
        campaign.operation_wall(materialize_wall)?;
        campaign.metric("A10", materialize_wall, Some(FILE_BYTES))?;
        let materialize_after = opened.fs.diagnostics().map_err(display_error)?;
        let materialize_engine = engine_delta(&materialize_before, &materialize_after)?;
        verify_read_only_engine(&materialize_engine)?;
        verify_operation_resources(&materialize_counters)?;
        if materialize_counters.workspace_materializations != 1 {
            return Err(format!("A10 sample {sample} lifecycle mismatch"));
        }
        let a10_post_started = Instant::now();
        let mut materialized = DigestSink::default();
        managed
            .read_to(FILE_PATH, &mut materialized)
            .map_err(display_error)?;
        let (materialized_bytes, materialized_digest) = materialized.finish();
        if materialized_bytes != FILE_BYTES || materialized_digest != master.raw_digest {
            return Err(format!("A10 sample {sample} native output mismatch"));
        }
        let a10_post_wall = a10_post_started.elapsed().as_nanos();
        campaign.postcheck_wall(a10_post_wall)?;
        let a10_resources = process_resources(&format!("A10/sample-{sample}"))?;

        let a_state = opened.fs.current_head("main").map_err(display_error)?;
        let noop_before = opened.fs.counter_snapshot().map_err(display_error)?;
        let noop_started = Instant::now();
        let noop_counters = managed.ensure_exact(&a_state).map_err(display_error)?;
        let noop_wall = noop_started.elapsed().as_nanos();
        campaign.operation_wall(noop_wall)?;
        campaign.metric("A11", noop_wall, None)?;
        let noop_after = opened.fs.diagnostics().map_err(display_error)?;
        let noop_engine = engine_delta(&noop_before, &noop_after)?;
        verify_exact_noop(&noop_counters, &noop_engine)?;
        let a11_resources = process_resources(&format!("A11/sample-{sample}"))?;

        let align_before = opened.fs.counter_snapshot().map_err(display_error)?;
        let align_started = Instant::now();
        let target = opened
            .fs
            .move_main(&a_state, root_b)
            .map_err(display_error)?;
        let align_wall = align_started.elapsed().as_nanos();
        campaign.operation_wall(align_wall)?;
        let align_after = opened.fs.diagnostics().map_err(display_error)?;
        let align_engine = engine_delta(&align_before, &align_after)?;
        verify_state_change(&align_engine, 1)?;
        let refresh_before = align_after;
        let refresh_started = Instant::now();
        let refresh_counters = managed.refresh(&target).map_err(display_error)?;
        let refresh_wall = refresh_started.elapsed().as_nanos();
        campaign.operation_wall(refresh_wall)?;
        campaign.metric("A12", refresh_wall, None)?;
        let refresh_after = opened.fs.diagnostics().map_err(display_error)?;
        campaign.store_database(refresh_after.database_bytes);
        let refresh_engine = engine_delta(&refresh_before, &refresh_after)?;
        verify_read_only_engine(&refresh_engine)?;
        verify_operation_resources(&refresh_counters)?;
        if !matches!(
            refresh_counters.native.route,
            Some(NativeRoute::ClonePatch | NativeRoute::InPlacePatch)
        ) || refresh_counters.native.patch_bytes != 4_096
            || refresh_counters.native.suffix_bytes_shifted != 0
            || refresh_counters.full_fallback_files != 0
        {
            return Err(format!(
                "A12 sample {sample} did not use an exact same-length patch route"
            ));
        }
        let a12_post_started = Instant::now();
        let mut refreshed = DigestSink::default();
        managed
            .read_to(FILE_PATH, &mut refreshed)
            .map_err(display_error)?;
        let (refreshed_bytes, refreshed_digest) = refreshed.finish();
        if refreshed_bytes != FILE_BYTES
            || refreshed_digest != target_digest
            || opened.fs.current_head("main").map_err(display_error)? != target
        {
            return Err(format!("A12 sample {sample} target mismatch"));
        }
        let a12_post_wall = a12_post_started.elapsed().as_nanos();
        campaign.postcheck_wall(a12_post_wall)?;
        let a12_resources = process_resources(&format!("A12/sample-{sample}"))?;
        managed.discard().map_err(display_error)?;
        campaign.data.last_q_terminal_bytes = Some(refresh_after.operation_q_current_bytes);
        drop(managed);
        drop(opened);
        let cleanup_wall = campaign.cleanup(attempt)?;
        let sequence_wall = complete_started.elapsed().as_nanos();

        campaign.row_with_resources(format!(
            "{{\"id\":\"A10\",\"sample\":{sample},\"cache\":\"cold-destination\",\"timing\":{{\"reset_ns\":{},\"open_ns\":{open_wall},\"operation_wall_ns\":{materialize_wall},\"attributed_wall_ns\":{materialize_wall},\"unattributed_wall_ns\":0,\"postcheck_ns\":{a10_post_wall},\"sequence_complete_sample_wall_ns\":{sequence_wall}}},\"oracle\":{{\"bytes\":{materialized_bytes},\"blake3\":\"{materialized_digest}\"}},\"clone\":{},\"operation_counters\":{},\"engine_delta\":{}}}",
            clone.wall_ns,
            clone_json(&clone),
            counters_json(&materialize_counters),
            engine_json(&materialize_engine),
        ), a10_resources)?;
        campaign.row_with_resources(format!(
            "{{\"id\":\"A11\",\"sample\":{sample},\"cache\":\"same-open-warm-or-unknown\",\"timing\":{{\"operation_wall_ns\":{noop_wall},\"attributed_wall_ns\":{noop_wall},\"unattributed_wall_ns\":0,\"sequence_complete_sample_wall_ns\":{sequence_wall}}},\"oracle\":{{\"exact_ref\":\"{}\",\"literal_zero_work\":true}},\"operation_counters\":{},\"engine_delta\":{}}}",
            a_state.root,
            counters_json(&noop_counters),
            engine_json(&noop_engine),
        ), a11_resources)?;
        campaign.row_with_resources(format!(
            "{{\"id\":\"A12\",\"sample\":{sample},\"cache\":\"same-open-warm-or-unknown\",\"timing\":{{\"ref_alignment_wall_ns\":{align_wall},\"operation_wall_ns\":{refresh_wall},\"attributed_wall_ns\":{refresh_wall},\"unattributed_wall_ns\":0,\"postcheck_ns\":{a12_post_wall},\"cleanup_ns\":{cleanup_wall},\"sequence_complete_sample_wall_ns\":{sequence_wall}}},\"oracle\":{{\"bytes\":{refreshed_bytes},\"blake3\":\"{refreshed_digest}\",\"target_root\":\"{}\"}},\"operation_counters\":{},\"alignment_engine_delta\":{},\"engine_delta\":{}}}",
            target.root,
            counters_json(&refresh_counters),
            engine_json(&align_engine),
            engine_json(&refresh_engine),
        ), a12_resources)?;
    }
    Ok(())
}

fn run_a13(campaign: &mut Campaign<'_>, master: &Master) -> EvalResult<()> {
    let expected = base(master, "read-reconstruct")?;
    let complete_started = Instant::now();
    let attempt = campaign.attempt("read-reconstruct", expected)?;
    let clone = attempt.clone.clone();
    let operation_started = Instant::now();
    let mut observations = Vec::with_capacity(11);
    let mut last_diagnostics = None;
    for _ in 0..11 {
        campaign.check_deadline()?;
        let started = Instant::now();
        let opened = attempt.open(expected, IntegrityMode::TrustedLocalDev)?;
        let head = opened.ref_state.clone();
        let wall = started.elapsed().as_nanos();
        if head != expected_ref(expected) {
            return Err("A13 reopened head mismatch".to_owned());
        }
        last_diagnostics = Some(opened.fs.diagnostics().map_err(display_error)?);
        observations.push(wall);
        campaign.metric("A13", wall, None)?;
        drop(opened);
    }
    let operation_wall = operation_started.elapsed().as_nanos();
    campaign.operation_wall(operation_wall)?;
    if let Some(diagnostics) = last_diagnostics {
        campaign.data.last_q_terminal_bytes = Some(diagnostics.operation_q_current_bytes);
    }
    let cleanup_wall = campaign.cleanup(attempt)?;
    campaign.row(format!(
        "{{\"id\":\"A13\",\"cache\":\"reopened-cache-unknown\",\"timing\":{{\"reset_ns\":{},\"operation_wall_ns\":{operation_wall},\"attributed_wall_ns\":{},\"unattributed_wall_ns\":{},\"cleanup_ns\":{cleanup_wall},\"complete_sample_wall_ns\":{}}},\"raw_observations_ns\":{},\"oracle\":{{\"observations\":11,\"exact_root\":\"{}\",\"native_workspace_scan\":false}},\"clone\":{}}}",
        clone.wall_ns,
        observations.iter().sum::<u128>(),
        timer_residual(operation_wall, observations.iter().sum::<u128>())?,
        complete_started.elapsed().as_nanos(),
        json_u128_array(&observations),
        expected.root,
        clone_json(&clone),
    ))
}

fn run_a14(campaign: &mut Campaign<'_>, master: &Master) -> EvalResult<()> {
    let expected = base(master, "history")?;
    let complete_started = Instant::now();
    let attempt = campaign.attempt("history", expected)?;
    let clone = attempt.clone.clone();
    let (opened, open_wall) = campaign.open(&attempt, expected)?;
    let edits = [
        history_edit(1, MIB),
        history_edit(2, 25 * MIB),
        history_edit(3, 50 * MIB),
        history_edit(4, 75 * MIB),
    ];
    let before = opened.fs.counter_snapshot().map_err(display_error)?;
    let mut states = vec![expected_ref(expected)];
    let mut observations = Vec::with_capacity(4);
    let mut counters = OperationDiagnostics::default();
    let operation_started = Instant::now();
    for edit in &edits {
        let started = Instant::now();
        let (state, observed) = opened
            .fs
            .replace_range_observed(
                states
                    .last()
                    .ok_or_else(|| "history state missing".to_owned())?,
                FILE_PATH,
                edit.start,
                edit.delete_len,
                std::io::Cursor::new(edit.replacement.as_slice()),
            )
            .map_err(display_error)?;
        observations.push(started.elapsed().as_nanos());
        counters = merge_counters(counters, observed)?;
        states.push(state);
    }
    let operation_wall = operation_started.elapsed().as_nanos();
    campaign.operation_wall(operation_wall)?;
    let after = opened.fs.diagnostics().map_err(display_error)?;
    campaign.store_database(after.database_bytes);
    let engine = engine_delta(&before, &after)?;
    verify_state_change(&engine, 4)?;
    let post_started = Instant::now();
    for revision in [0_usize, 1, 2, 4] {
        let start = match revision {
            0 => 0,
            1 => edits[0].start,
            2 => edits[1].start,
            _ => edits[3].start,
        };
        let mut actual = Vec::new();
        opened
            .fs
            .read_range(
                states[revision].root,
                FILE_PATH,
                start..start + RANDOM_RANGE_BYTES,
                &mut actual,
            )
            .map_err(display_error)?;
        if actual != history_expected_range(revision, start, RANDOM_RANGE_BYTES as usize, &edits)? {
            return Err(format!("A14 revision {revision} range mismatch"));
        }
    }
    if opened.fs.current_head("main").map_err(display_error)? != states[4] {
        return Err("A14 terminal RefState mismatch".to_owned());
    }
    let post_wall = post_started.elapsed().as_nanos();
    campaign.postcheck_wall(post_wall)?;
    for wall in &observations {
        campaign.metric("A14/edit", *wall, None)?;
    }
    campaign.data.last_q_terminal_bytes = Some(after.operation_q_current_bytes);
    drop(opened);
    let cleanup_wall = campaign.cleanup(attempt)?;
    campaign.row(format!(
        "{{\"id\":\"A14\",\"cache\":\"same-open-warm-or-unknown\",\"timing\":{{\"reset_ns\":{},\"open_ns\":{open_wall},\"operation_wall_ns\":{operation_wall},\"attributed_wall_ns\":{},\"unattributed_wall_ns\":{},\"postcheck_ns\":{post_wall},\"cleanup_ns\":{cleanup_wall},\"complete_sample_wall_ns\":{}}},\"raw_revision_observations_ns\":{},\"oracle\":{{\"direct_revisions_read\":[0,1,2,4],\"no_replay\":true,\"terminal_root\":\"{}\"}},\"store_growth_bytes\":{},\"clone\":{},\"operation_counters\":{},\"engine_delta\":{}}}",
        clone.wall_ns,
        observations.iter().sum::<u128>(),
        timer_residual(operation_wall, observations.iter().sum::<u128>())?,
        complete_started.elapsed().as_nanos(),
        json_u128_array(&observations),
        states[4].root,
        option_growth_json(before.database_bytes, after.database_bytes),
        clone_json(&clone),
        counters_json(&counters),
        engine_json(&engine),
    ))
}

fn run_a15(campaign: &mut Campaign<'_>, master: &Master) -> EvalResult<()> {
    let expected = base(master, "overwrite")?;
    let positions = [4_096, FILE_BYTES / 2 - 2_048, FILE_BYTES - 8_192];
    let labels = ["early", "middle", "late"];
    let mut locality = Vec::new();
    for (index, (position, label)) in positions.into_iter().zip(labels).enumerate() {
        let complete_started = Instant::now();
        let attempt = campaign.attempt("overwrite", expected)?;
        let clone = attempt.clone.clone();
        let (opened, open_wall) = campaign.open(&attempt, expected)?;
        let replacement = edit_bytes(0x51 + index as u8, 4_096);
        let case = EditCase {
            id: "A15",
            base: "overwrite",
            base_len: FILE_BYTES,
            start: position,
            delete_len: 4_096,
            replacement,
        };
        let before = opened.fs.counter_snapshot().map_err(display_error)?;
        let operation_started = Instant::now();
        let (state, counters) = opened
            .fs
            .replace_range_observed(
                &expected_ref(expected),
                FILE_PATH,
                position,
                4_096,
                std::io::Cursor::new(case.replacement.as_slice()),
            )
            .map_err(display_error)?;
        let operation_wall = operation_started.elapsed().as_nanos();
        campaign.operation_wall(operation_wall)?;
        let after = opened.fs.diagnostics().map_err(display_error)?;
        campaign.store_database(after.database_bytes);
        let engine = engine_delta(&before, &after)?;
        verify_state_change(&engine, 1)?;
        verify_logical_locality(&counters, 4_096)?;
        locality.push((
            counters.rope.cdc_bytes_scanned,
            counters.rope.payload_bytes_read,
            counters.rope.payload_bytes_written,
            counters.namespace.nodes_created,
        ));
        let post_started = Instant::now();
        let (bytes, digest, _) = canonical_digest(&opened.fs, state.root)?;
        if bytes != FILE_BYTES || digest != splice_digest(&case)? {
            return Err(format!("A15 {label} output mismatch"));
        }
        verify_old_root_range(&opened.fs, expected.root, &case)?;
        let post_wall = post_started.elapsed().as_nanos();
        campaign.postcheck_wall(post_wall)?;
        campaign.metric("A15", operation_wall, None)?;
        drop(opened);
        let cleanup_wall = campaign.cleanup(attempt)?;
        campaign.row(format!(
            "{{\"id\":\"A15\",\"position\":\"{label}\",\"timing\":{{\"reset_ns\":{},\"open_ns\":{open_wall},\"operation_wall_ns\":{operation_wall},\"attributed_wall_ns\":{operation_wall},\"unattributed_wall_ns\":0,\"postcheck_ns\":{post_wall},\"cleanup_ns\":{cleanup_wall},\"complete_sample_wall_ns\":{}}},\"operand\":{},\"oracle\":{{\"bytes\":{bytes},\"blake3\":\"{digest}\",\"old_root_readable\":true}},\"locality_evidence\":{},\"clone\":{},\"operation_counters\":{},\"engine_delta\":{}}}",
            clone.wall_ns,
            complete_started.elapsed().as_nanos(),
            edit_case_json(&case)?,
            locality_evidence_json(),
            clone_json(&clone),
            counters_json(&counters),
            engine_json(&engine),
        ))?;
    }
    if locality
        .iter()
        .any(|value| value.0 != 4_096 || value.3 != 0)
        || locality.windows(2).any(|pair| pair[0] != pair[1])
    {
        return Err("A15 locality counters vary outside tree-path details".to_owned());
    }
    Ok(())
}

fn run_a17(campaign: &mut Campaign<'_>, master: &Master) -> EvalResult<()> {
    let expected = base(master, "overwrite")?;
    let complete_started = Instant::now();
    let attempt = campaign.attempt("overwrite", expected)?;
    let clone = attempt.clone.clone();
    let (opened, open_wall) = campaign.open(&attempt, expected)?;
    let prepare_started = Instant::now();
    let (mut managed, prepare_counters) = opened
        .fs
        .materialize_managed_observed(expected.root)
        .map_err(display_error)?;
    let prepare_wall = prepare_started.elapsed().as_nanos();
    verify_operation_resources(&prepare_counters)?;
    campaign.data.managed_prepare_wall_ns = campaign
        .data
        .managed_prepare_wall_ns
        .checked_add(prepare_wall)
        .ok_or_else(|| "managed prepare timer overflow".to_owned())?;
    if prepare_counters.workspace_materializations != 1 {
        return Err("A17 must start with exactly one materialization".to_owned());
    }
    let before = opened.fs.counter_snapshot().map_err(display_error)?;
    let position = FILE_BYTES / 2 - 2_048;
    let mut states = vec![expected_ref(expected)];
    let mut edit_observations = Vec::with_capacity(100);
    let mut checkpoint_observations = Vec::with_capacity(100);
    let mut counters = OperationDiagnostics::default();
    let operation_started = Instant::now();
    for iteration in 1..=100_u8 {
        campaign.check_deadline()?;
        let replacement = edit_bytes(iteration, 4_096);
        let edit_started = Instant::now();
        let edit_counters = managed
            .replace_observed(FILE_PATH, position, 4_096, &replacement)
            .map_err(display_error)?;
        let edit_wall = edit_started.elapsed().as_nanos();
        let checkpoint_started = Instant::now();
        let (state, checkpoint_counters) = managed.checkpoint_observed().map_err(display_error)?;
        let checkpoint_wall = checkpoint_started.elapsed().as_nanos();
        if checkpoint_counters.descriptor_resets != 1
            || checkpoint_counters.descriptor_spool_bytes_terminal != 0
        {
            return Err(format!(
                "A17 checkpoint {iteration} did not reset its descriptor spool"
            ));
        }
        counters = merge_counters(counters, edit_counters)?;
        counters = merge_counters(counters, checkpoint_counters)?;
        edit_observations.push(edit_wall);
        checkpoint_observations.push(checkpoint_wall);
        campaign.metric("A17/checkpoint", checkpoint_wall, None)?;
        campaign.metric(
            "A17/edit-plus-checkpoint",
            edit_wall + checkpoint_wall,
            None,
        )?;
        states.push(state);
    }
    let operation_wall = operation_started.elapsed().as_nanos();
    campaign.operation_wall(operation_wall)?;
    let after = opened.fs.diagnostics().map_err(display_error)?;
    campaign.store_database(after.database_bytes);
    let engine = engine_delta(&before, &after)?;
    verify_state_change(&engine, 100)?;
    if counters.descriptor_resets != 100
        || counters.workspace_reuses != 100
        || counters.workspace_materializations != 0
        || counters.rematerializations != 0
    {
        return Err("A17 reuse/rematerialization/descriptor equation failed".to_owned());
    }
    let post_started = Instant::now();
    for revision in [0_usize, 1, 50, 100] {
        let mut actual = Vec::new();
        opened
            .fs
            .read_range(
                states[revision].root,
                FILE_PATH,
                position..position + 4_096,
                &mut actual,
            )
            .map_err(display_error)?;
        let wanted = if revision == 0 {
            expected_bytes(position, 4_096)?
        } else {
            edit_bytes(revision as u8, 4_096)
        };
        if actual != wanted {
            return Err(format!("A17 retained revision {revision} mismatch"));
        }
    }
    let mut terminal = DigestSink::default();
    managed
        .read_to(FILE_PATH, &mut terminal)
        .map_err(display_error)?;
    let (terminal_bytes, terminal_digest) = terminal.finish();
    let terminal_case = EditCase {
        id: "A17",
        base: "overwrite",
        base_len: FILE_BYTES,
        start: position,
        delete_len: 4_096,
        replacement: edit_bytes(100, 4_096),
    };
    if terminal_bytes != FILE_BYTES
        || terminal_digest != splice_digest(&terminal_case)?
        || opened.fs.current_head("main").map_err(display_error)? != states[100]
    {
        return Err("A17 terminal root/bytes mismatch".to_owned());
    }
    let post_wall = post_started.elapsed().as_nanos();
    campaign.postcheck_wall(post_wall)?;
    managed.discard().map_err(display_error)?;
    drop(managed);
    let terminal_diagnostics = opened.fs.diagnostics().map_err(display_error)?;
    if terminal_diagnostics.operation_q_current_bytes != 0 {
        return Err("A17 terminal operation Q is nonzero".to_owned());
    }
    campaign.data.last_q_terminal_bytes = Some(terminal_diagnostics.operation_q_current_bytes);
    drop(opened);
    let cleanup_wall = campaign.cleanup(attempt)?;
    campaign.row(format!(
        "{{\"id\":\"A17\",\"cache\":\"same-open-warm-or-unknown\",\"timing\":{{\"reset_ns\":{},\"open_ns\":{open_wall},\"managed_prepare_wall_ns\":{prepare_wall},\"operation_wall_ns\":{operation_wall},\"attributed_wall_ns\":{},\"unattributed_wall_ns\":{},\"postcheck_ns\":{post_wall},\"cleanup_ns\":{cleanup_wall},\"complete_sample_wall_ns\":{}}},\"raw_edit_observations_ns\":{},\"raw_checkpoint_observations_ns\":{},\"oracle\":{{\"checkpoints\":100,\"selected_roots\":[0,1,50,100],\"terminal_root\":\"{}\",\"terminal_bytes\":{terminal_bytes},\"terminal_blake3\":\"{terminal_digest}\",\"initial_materializations\":1,\"checkpoint_workspace_reuses\":100,\"rematerializations\":0,\"rematerialization_evidence\":\"one-initial-materialization-plus-100-retained-workspace-reuses\"}},\"clone\":{},\"managed_prepare_counters\":{},\"operation_counters\":{},\"engine_delta\":{},\"terminal_operation_q_bytes\":{}}}",
        clone.wall_ns,
        edit_observations.iter().sum::<u128>() + checkpoint_observations.iter().sum::<u128>(),
        timer_residual(
            operation_wall,
            edit_observations.iter().sum::<u128>()
                + checkpoint_observations.iter().sum::<u128>(),
        )?,
        complete_started.elapsed().as_nanos(),
        json_u128_array(&edit_observations),
        json_u128_array(&checkpoint_observations),
        states[100].root,
        clone_json(&clone),
        counters_json(&prepare_counters),
        counters_json(&counters),
        engine_json(&engine),
        terminal_diagnostics.operation_q_current_bytes,
    ))
}

fn expected_ref(base: &BaseManifest) -> RefState {
    RefState {
        name: "main".to_owned(),
        generation: base.generation,
        root: base.root,
    }
}

fn merge_counters(
    left: OperationDiagnostics,
    right: OperationDiagnostics,
) -> EvalResult<OperationDiagnostics> {
    let merged = left.merge(right).map_err(display_error)?;
    verify_operation_resources(&merged)?;
    Ok(merged)
}

fn canonical_digest(fs: &LayerFs, root: RootId) -> EvalResult<(u64, String, OperationDiagnostics)> {
    let mut sink = DigestSink::default();
    let counters = fs
        .read_to(root, FILE_PATH, &mut sink)
        .map_err(display_error)?;
    let (bytes, digest) = sink.finish();
    Ok((bytes, digest, counters))
}

fn edit_result_len(case: &EditCase) -> EvalResult<u64> {
    case.base_len
        .checked_sub(case.delete_len)
        .and_then(|value| value.checked_add(case.replacement.len() as u64))
        .filter(|value| *value <= FILE_BYTES)
        .ok_or_else(|| format!("{} result violates the 100 MiB ceiling", case.id))
}

fn splice_digest(case: &EditCase) -> EvalResult<String> {
    let suffix = case
        .base_len
        .checked_sub(case.start)
        .and_then(|value| value.checked_sub(case.delete_len))
        .ok_or_else(|| format!("{} splice is outside its base", case.id))?;
    let mut sink = DigestSink::default();
    stream_expected(0, case.start, &mut sink)?;
    sink.write_all(&case.replacement).map_err(io_error)?;
    stream_expected(case.start + case.delete_len, suffix, &mut sink)?;
    let (bytes, digest) = sink.finish();
    if bytes != edit_result_len(case)? {
        return Err(format!("{} oracle length mismatch", case.id));
    }
    Ok(digest)
}

fn verify_old_root_range(fs: &LayerFs, root: RootId, case: &EditCase) -> EvalResult<()> {
    let available = case
        .base_len
        .checked_sub(case.start)
        .ok_or_else(|| format!("{} old-root range is outside its base", case.id))?;
    let length = available.min(4_096) as usize;
    if length == 0 {
        return Ok(());
    }
    let mut actual = Vec::new();
    fs.read_range(
        root,
        FILE_PATH,
        case.start..case.start + length as u64,
        &mut actual,
    )
    .map_err(display_error)?;
    if actual != expected_bytes(case.start, length)? {
        return Err(format!("{} old root changed", case.id));
    }
    Ok(())
}

fn history_edit(tag: u8, start: u64) -> EditCase {
    EditCase {
        id: "A14",
        base: "history",
        base_len: FILE_BYTES,
        start,
        delete_len: 4_096,
        replacement: edit_bytes(0x60 + tag, 4_096),
    }
}

fn history_expected_range(
    revision: usize,
    start: u64,
    length: usize,
    edits: &[EditCase],
) -> EvalResult<Vec<u8>> {
    let mut output = expected_bytes(start, length)?;
    let end = start + length as u64;
    for edit in edits.iter().take(revision) {
        let edit_end = edit.start + edit.replacement.len() as u64;
        let overlap_start = start.max(edit.start);
        let overlap_end = end.min(edit_end);
        if overlap_start < overlap_end {
            let output_start = (overlap_start - start) as usize;
            let replacement_start = (overlap_start - edit.start) as usize;
            let overlap = (overlap_end - overlap_start) as usize;
            output[output_start..output_start + overlap]
                .copy_from_slice(&edit.replacement[replacement_start..replacement_start + overlap]);
        }
    }
    Ok(output)
}

fn engine_delta(before: &Diagnostics, after: &Diagnostics) -> EvalResult<EngineDelta> {
    macro_rules! delta {
        ($field:ident) => {
            after
                .$field
                .checked_sub(before.$field)
                .ok_or_else(|| format!("engine counter {} moved backwards", stringify!($field)))?
        };
    }
    Ok(EngineDelta {
        transactions_started: delta!(transactions_started),
        transactions_committed: delta!(transactions_committed),
        transactions_rolled_back: delta!(transactions_rolled_back),
        statements: delta!(statements),
        objects_validated: delta!(objects_validated),
        objects_created: delta!(objects_created),
        objects_reused: delta!(objects_reused),
        object_bytes_read: delta!(object_bytes_read),
        object_bytes_written: delta!(object_bytes_written),
        range_bytes_requested: delta!(range_bytes_requested),
        range_bytes_returned: delta!(range_bytes_returned),
        root_verifications: delta!(root_verifications),
        root_verification_objects: delta!(root_verification_objects),
        root_verification_bytes: delta!(root_verification_bytes),
        fetched_rows: delta!(fetched_rows),
        fetched_row_authentication_passes: delta!(fetched_row_authentication_passes),
        fetched_row_role_decode_passes: delta!(fetched_row_role_decode_passes),
        new_object_authentication_passes: delta!(new_object_authentication_passes),
        incumbent_authentication_passes: delta!(incumbent_authentication_passes),
        payload_batch_queries: delta!(payload_batch_queries),
        payload_batch_references: delta!(payload_batch_references),
        payload_batch_session_maximum: after.payload_batch_maximum,
        put_lookup_statements: delta!(put_lookup_statements),
        put_insert_statements: delta!(put_insert_statements),
        created_rows: delta!(created_rows),
        reused_rows: delta!(reused_rows),
        publication_commits: delta!(publication_commits),
        publication_closure_passes: delta!(publication_closure_passes),
        namespace_graph_verification_passes: delta!(namespace_graph_verification_passes),
        scratch_tables: delta!(scratch_tables),
        scratch_statements: delta!(scratch_statements),
        scratch_rows: delta!(scratch_rows),
        scratch_session_high_water_bytes: after.scratch_high_water_bytes,
    })
}

fn verify_engine_equations(delta: &EngineDelta) -> EvalResult<()> {
    if delta.fetched_rows != delta.fetched_row_authentication_passes
        || delta.fetched_rows != delta.fetched_row_role_decode_passes
    {
        return Err(format!(
            "fetched/auth/decode equation failed: {}/{}/{}",
            delta.fetched_rows,
            delta.fetched_row_authentication_passes,
            delta.fetched_row_role_decode_passes
        ));
    }
    if delta.payload_batch_session_maximum > 64 {
        return Err(format!(
            "payload batch maximum {} exceeds 64",
            delta.payload_batch_session_maximum
        ));
    }
    if delta.scratch_session_high_water_bytes > 8 * MIB {
        return Err(format!(
            "engine scratch high-water {} exceeds 8 MiB",
            delta.scratch_session_high_water_bytes
        ));
    }
    Ok(())
}

fn verify_operation_resources(counters: &OperationDiagnostics) -> EvalResult<()> {
    if counters.operation_q_high_water_bytes > 8 * MIB
        || counters.operation_q_terminal_bytes != 0
        || counters.scratch_high_water_bytes > 8 * MIB
        || counters.plan_scratch_high_water_bytes > 8 * MIB
    {
        return Err(format!(
            "operation resource gate failed: Q high/terminal={}/{}, scratch/plan={}/{}",
            counters.operation_q_high_water_bytes,
            counters.operation_q_terminal_bytes,
            counters.scratch_high_water_bytes,
            counters.plan_scratch_high_water_bytes
        ));
    }
    Ok(())
}

fn verify_direct_read(counters: &OperationDiagnostics) -> EvalResult<()> {
    verify_operation_resources(counters)?;
    if counters.native != Default::default()
        || counters.rope.payload_bytes_written != 0
        || counters.rope.cdc_bytes_scanned != 0
    {
        return Err("direct canonical read performed native/write/CDC work".to_owned());
    }
    Ok(())
}

fn verify_read_only_engine(delta: &EngineDelta) -> EvalResult<()> {
    verify_engine_equations(delta)?;
    if delta.transactions_started != 0
        || delta.transactions_committed != 0
        || delta.publication_commits != 0
    {
        return Err("read-only operation performed a writer transaction/COMMIT".to_owned());
    }
    Ok(())
}

fn verify_state_change(delta: &EngineDelta, expected: u64) -> EvalResult<()> {
    verify_engine_equations(delta)?;
    if delta.transactions_started != expected
        || delta.transactions_committed != expected
        || delta.transactions_rolled_back != 0
        || delta.publication_commits != expected
    {
        return Err(format!(
            "state-change transaction equation failed: started={}, committed={}, rolled_back={}, publication_commits={}, expected={expected}",
            delta.transactions_started,
            delta.transactions_committed,
            delta.transactions_rolled_back,
            delta.publication_commits
        ));
    }
    Ok(())
}

fn verify_logical_locality(
    counters: &OperationDiagnostics,
    replacement_bytes: u64,
) -> EvalResult<()> {
    verify_operation_resources(counters)?;
    let (cdc_bytes_scanned, payload_bytes_read, payload_bytes_written) = content_rope(counters)?;
    if cdc_bytes_scanned > replacement_bytes
        || payload_bytes_read != 0
        || payload_bytes_written > replacement_bytes
        || counters.namespace.nodes_created != 0
        || counters.native != Default::default()
    {
        return Err(format!(
            "logical locality equation failed: cdc={}, payload_read={}, payload_write={}, directory_nodes={}, native={}",
            cdc_bytes_scanned,
            payload_bytes_read,
            payload_bytes_written,
            counters.namespace.nodes_created,
            native_json(counters)
        ));
    }
    Ok(())
}

fn verify_native_edit_shape(counters: &OperationDiagnostics, case: &EditCase) -> EvalResult<()> {
    verify_operation_resources(counters)?;
    let (cdc_bytes_scanned, payload_bytes_read, payload_bytes_written) = content_rope(counters)?;
    if cdc_bytes_scanned > case.replacement.len() as u64
        || payload_bytes_read != 0
        || payload_bytes_written > case.replacement.len() as u64
        || counters.namespace.nodes_created != 0
    {
        return Err(format!("{} canonical locality equation failed", case.id));
    }
    let count_change = case.delete_len != case.replacement.len() as u64;
    if !count_change {
        if !matches!(
            counters.native.route,
            Some(NativeRoute::ClonePatch | NativeRoute::InPlacePatch)
        ) || counters.native.suffix_bytes_shifted != 0
            || counters.native.patch_bytes != case.replacement.len() as u64
        {
            return Err(format!("{} native same-size route mismatch", case.id));
        }
    } else {
        match counters.native.route {
            Some(NativeRoute::InPlaceShift) => {
                let suffix = case
                    .base_len
                    .checked_sub(case.start + case.delete_len)
                    .ok_or_else(|| "native suffix equation underflow".to_owned())?;
                let transfer = suffix
                    .checked_mul(2)
                    .and_then(|value| value.checked_add(case.replacement.len() as u64))
                    .ok_or_else(|| "native suffix equation overflow".to_owned())?;
                if counters.native.suffix_bytes_shifted != suffix
                    || counters.native.bytes_read + counters.native.bytes_written != transfer
                {
                    return Err(format!(
                        "{} in-place shift equation failed: S={suffix}, transfer={transfer}",
                        case.id
                    ));
                }
            }
            Some(NativeRoute::FullFallback) => {
                if counters.full_fallback_files != 1 {
                    return Err(format!("{} full fallback was not counted", case.id));
                }
            }
            route => {
                return Err(format!(
                    "{} count-changing native route is {:?}",
                    case.id, route
                ));
            }
        }
    }
    Ok(())
}

fn verify_exact_noop(counters: &OperationDiagnostics, engine: &EngineDelta) -> EvalResult<()> {
    verify_read_only_engine(engine)?;
    verify_operation_resources(counters)?;
    if counters.native.route != Some(NativeRoute::ExactNoop)
        || counters.rope.payload_bytes_read != 0
        || counters.rope.payload_bytes_written != 0
        || counters.rope.cdc_bytes_scanned != 0
        || counters.native.bytes_read != 0
        || counters.native.bytes_written != 0
        || engine.object_bytes_read != 0
        || engine.object_bytes_written != 0
    {
        return Err("managed exact no-op performed payload/native/CDC/write work".to_owned());
    }
    Ok(())
}

fn content_rope(counters: &OperationDiagnostics) -> EvalResult<(u64, u64, u64)> {
    Ok((
        counters
            .rope
            .cdc_bytes_scanned
            .checked_sub(counters.metadata_rope.cdc_bytes_scanned)
            .ok_or_else(|| "metadata CDC counter exceeds aggregate rope counter".to_owned())?,
        counters
            .rope
            .payload_bytes_read
            .checked_sub(counters.metadata_rope.payload_bytes_read)
            .ok_or_else(|| "metadata read counter exceeds aggregate rope counter".to_owned())?,
        counters
            .rope
            .payload_bytes_written
            .checked_sub(counters.metadata_rope.payload_bytes_written)
            .ok_or_else(|| "metadata write counter exceeds aggregate rope counter".to_owned())?,
    ))
}

fn locality_evidence_json() -> &'static str {
    "{\"unaffected_suffix_payload_reads\":0,\"unaffected_suffix_payload_writes\":0,\"derivation\":\"total-content-payload-read-zero-and-content-payload-write-bounded-by-replacement\"}"
}

fn clone_json(value: &CloneReceipt) -> String {
    format!(
        "{{\"evidence\":\"APFSCloneReturnPlusSealedMasterCustodyNotPerResetFullRehash\",\"reset_wall_ns\":{},\"clone_return_wall_ns\":{},\"source_logical_bytes\":{},\"destination_logical_bytes\":{},\"source_allocated_bytes\":{},\"destination_allocated_bytes\":{},\"distinct_regular_inodes\":{},\"clone_id\":{}}}",
        value.wall_ns,
        value.clone_wall_ns,
        value.source_logical_bytes,
        value.destination_logical_bytes,
        value.source_allocated_bytes,
        value.destination_allocated_bytes,
        value.distinct_regular_inodes,
        value.clone_id,
    )
}

fn edit_case_json(value: &EditCase) -> EvalResult<String> {
    Ok(format!(
        "{{\"base_bytes\":{},\"offset\":{},\"delete_bytes\":{},\"replacement_bytes\":{},\"result_bytes\":{}}}",
        value.base_len,
        value.start,
        value.delete_len,
        value.replacement.len(),
        edit_result_len(value)?,
    ))
}

fn native_json(value: &OperationDiagnostics) -> String {
    let native = &value.native;
    format!(
        concat!(
            "{{\"route\":{},\"bytes_read\":{},\"bytes_written\":{},\"patch_bytes\":{},",
            "\"suffix_bytes_shifted\":{},\"clone_attempts\":{},\"clone_successes\":{},",
            "\"clone_fallbacks\":{},\"temp_calls\":{},\"sync_calls\":{},",
            "\"rename_calls\":{},\"replace_calls\":{},\"metadata_calls\":{},",
            "\"create_calls\":{},\"remove_calls\":{},\"hard_link_calls\":{}}}"
        ),
        native_route_json(native.route),
        native.bytes_read,
        native.bytes_written,
        native.patch_bytes,
        native.suffix_bytes_shifted,
        native.clone_attempts,
        native.clone_successes,
        native.clone_fallbacks,
        native.temp_calls,
        native.sync_calls,
        native.rename_calls,
        native.replace_calls,
        native.metadata_calls,
        native.create_calls,
        native.remove_calls,
        native.hard_link_calls,
    )
}

fn counters_json(value: &OperationDiagnostics) -> String {
    format!(
        concat!(
            "{{\"rope\":{{\"payload_bytes_read\":{},\"payload_bytes_written\":{},",
            "\"cdc_bytes_scanned\":{},\"chunks_emitted\":{},\"nodes_read\":{},\"nodes_emitted\":{}}},",
            "\"metadata_rope\":{{\"payload_bytes_read\":{},\"payload_bytes_written\":{},",
            "\"cdc_bytes_scanned\":{},\"chunks_emitted\":{},\"nodes_read\":{},\"nodes_emitted\":{}}},",
            "\"namespace\":{{\"nodes_read\":{},\"nodes_emitted\":{}}},",
            "\"inode_table\":{{\"nodes_read\":{},\"nodes_emitted\":{}}},",
            "\"native\":{},\"workspace_materializations\":{},\"workspace_reuses\":{},",
            "\"rematerializations\":{},\"descriptor_resets\":{},\"root_diff_nodes\":{},",
            "\"changed_paths\":{},\"full_fallback_files\":{},\"plan_rows\":{},",
            "\"plan_scratch_high_water_bytes\":{},\"current_digest_bytes\":{},",
            "\"uncached_prior_digest_bytes\":{},\"changed_current_cdc_bytes\":{},",
            "\"unchanged_file_roots_reused\":{},\"authority_full_scans\":{},",
            "\"scratch_tables\":{},\"scratch_statements\":{},\"scratch_rows\":{},",
            "\"scratch_high_water_bytes\":{},\"operation_q_current_bytes\":{},",
            "\"operation_q_high_water_bytes\":{},\"operation_q_terminal_bytes\":{},",
            "\"owned_temp_current\":{},\"owned_temp_terminal\":{},",
            "\"descriptor_spool_bytes_current\":{},\"descriptor_spool_bytes_terminal\":{}}}"
        ),
        value.rope.payload_bytes_read,
        value.rope.payload_bytes_written,
        value.rope.cdc_bytes_scanned,
        value.rope.chunks_created,
        value.rope.nodes_read,
        value.rope.nodes_created,
        value.metadata_rope.payload_bytes_read,
        value.metadata_rope.payload_bytes_written,
        value.metadata_rope.cdc_bytes_scanned,
        value.metadata_rope.chunks_created,
        value.metadata_rope.nodes_read,
        value.metadata_rope.nodes_created,
        value.namespace.nodes_read,
        value.namespace.nodes_created,
        value.inode_table.nodes_read,
        value.inode_table.nodes_created,
        native_json(value),
        value.workspace_materializations,
        value.workspace_reuses,
        value.rematerializations,
        value.descriptor_resets,
        value.root_diff_nodes,
        value.changed_paths,
        value.full_fallback_files,
        value.plan_rows,
        value.plan_scratch_high_water_bytes,
        value.current_digest_bytes,
        value.uncached_prior_digest_bytes,
        value.changed_current_cdc_bytes,
        value.unchanged_file_roots_reused,
        value.authority_full_scans,
        value.scratch_tables,
        value.scratch_statements,
        value.scratch_rows,
        value.scratch_high_water_bytes,
        value.operation_q_current_bytes,
        value.operation_q_high_water_bytes,
        value.operation_q_terminal_bytes,
        value.owned_temp_current,
        value.owned_temp_terminal,
        value.descriptor_spool_bytes_current,
        value.descriptor_spool_bytes_terminal,
    )
}

fn engine_json(value: &EngineDelta) -> String {
    format!(
        concat!(
            "{{\"transactions_started\":{},\"transactions_committed\":{},",
            "\"transactions_rolled_back\":{},\"statements\":{},\"objects_validated\":{},",
            "\"objects_created\":{},\"objects_reused\":{},\"object_bytes_read\":{},",
            "\"object_bytes_written\":{},\"range_bytes_requested\":{},",
            "\"range_bytes_returned\":{},\"root_verifications\":{},",
            "\"root_verification_objects\":{},\"root_verification_bytes\":{},",
            "\"fetched_rows\":{},\"fetched_row_authentication_passes\":{},",
            "\"fetched_row_role_decode_passes\":{},\"new_object_authentication_passes\":{},",
            "\"incumbent_authentication_passes\":{},\"payload_batch_queries\":{},",
            "\"payload_batch_references\":{},\"payload_batch_session_maximum\":{},",
            "\"put_lookup_statements\":{},\"put_insert_statements\":{},",
            "\"created_rows\":{},\"reused_rows\":{},\"publication_commits\":{},",
            "\"publication_closure_passes\":{},\"namespace_graph_verification_passes\":{},",
            "\"scratch_tables\":{},\"scratch_statements\":{},\"scratch_rows\":{},",
            "\"scratch_session_high_water_bytes\":{}}}"
        ),
        value.transactions_started,
        value.transactions_committed,
        value.transactions_rolled_back,
        value.statements,
        value.objects_validated,
        value.objects_created,
        value.objects_reused,
        value.object_bytes_read,
        value.object_bytes_written,
        value.range_bytes_requested,
        value.range_bytes_returned,
        value.root_verifications,
        value.root_verification_objects,
        value.root_verification_bytes,
        value.fetched_rows,
        value.fetched_row_authentication_passes,
        value.fetched_row_role_decode_passes,
        value.new_object_authentication_passes,
        value.incumbent_authentication_passes,
        value.payload_batch_queries,
        value.payload_batch_references,
        value.payload_batch_session_maximum,
        value.put_lookup_statements,
        value.put_insert_statements,
        value.created_rows,
        value.reused_rows,
        value.publication_commits,
        value.publication_closure_passes,
        value.namespace_graph_verification_passes,
        value.scratch_tables,
        value.scratch_statements,
        value.scratch_rows,
        value.scratch_session_high_water_bytes,
    )
}

fn native_route_json(route: Option<NativeRoute>) -> String {
    route.map_or_else(|| "null".to_owned(), |route| format!("\"{route:?}\""))
}

fn option_u64_json(value: Option<u64>) -> String {
    value.map_or_else(|| "\"Unavailable\"".to_owned(), |value| value.to_string())
}

fn observed_u64_json(observed: bool, value: u64) -> String {
    if observed {
        value.to_string()
    } else {
        "\"Unavailable\"".to_owned()
    }
}

fn option_growth_json(before: Option<u64>, after: Option<u64>) -> String {
    match (before, after) {
        (Some(before), Some(after)) if after >= before => (after - before).to_string(),
        _ => "\"Unavailable\"".to_owned(),
    }
}

fn json_u64_array(values: &[u64]) -> String {
    format!(
        "[{}]",
        values
            .iter()
            .map(u64::to_string)
            .collect::<Vec<_>>()
            .join(",")
    )
}

fn timer_residual(total: u128, attributed: u128) -> EvalResult<u128> {
    total
        .checked_sub(attributed)
        .ok_or_else(|| "attributed wall exceeds enclosing timer".to_owned())
}

#[derive(Clone, Debug, Default)]
struct TerminalResources {
    observed: bool,
    observation_error: Option<String>,
    fd_baseline: u64,
    fd_terminal: u64,
    attempt_residue: u64,
    open_store_connections: u64,
    current_rss_bytes: u64,
    maximum_rss_bytes: u64,
}

struct Statistics {
    sorted: Vec<u128>,
    minimum: u128,
    maximum: u128,
    range: u128,
    p50: u128,
    p95: u128,
    operation_wall: u128,
}

fn terminal_resources(fd_baseline: u64) -> EvalResult<TerminalResources> {
    Ok(TerminalResources {
        observed: true,
        observation_error: None,
        fd_baseline,
        fd_terminal: fd_count()?,
        attempt_residue: attempt_residue_count()?,
        open_store_connections: open_store_connection_count()?,
        current_rss_bytes: current_rss_bytes()?,
        maximum_rss_bytes: maximum_rss_bytes()?,
    })
}

fn append_a16(
    campaign: &mut Campaign<'_>,
    terminal: &TerminalResources,
    master_unchanged: bool,
) -> EvalResult<Option<String>> {
    let failed = !terminal.observed
        || terminal.fd_terminal != terminal.fd_baseline
        || terminal.attempt_residue != 0
        || terminal.open_store_connections != 0
        || terminal.maximum_rss_bytes > 67_108_864
        || campaign.data.last_q_terminal_bytes != Some(0)
        || !master_unchanged;
    let error = failed.then(|| match terminal.observation_error.as_deref() {
        Some(observation) => format!(
            "A16 terminal resource observation unavailable: {observation}; Q {:?}, master unchanged {}",
            campaign.data.last_q_terminal_bytes, master_unchanged,
        ),
        None => format!(
            "A16 terminal resource equation failed: fd {}/{}, residue {}, connections {}, current RSS {}, peak RSS {}, Q {:?}, master unchanged {}",
            terminal.fd_terminal,
            terminal.fd_baseline,
            terminal.attempt_residue,
            terminal.open_store_connections,
            terminal.current_rss_bytes,
            terminal.maximum_rss_bytes,
            campaign.data.last_q_terminal_bytes,
            master_unchanged,
        ),
    });
    let resources = ProcessResources {
        operation: "A16".to_owned(),
        observed: terminal.observed,
        current_rss_bytes: terminal.current_rss_bytes,
        process_peak_rss_bytes: terminal.maximum_rss_bytes,
    };
    campaign.row_with_resources(
        format!(
            "{{\"id\":\"A16\",\"gate_status\":\"{}\",\"gate_error\":{},\"terminal\":{{\"observed\":{},\"observation_error\":{},\"operation_q_bytes\":{},\"fd_baseline\":{},\"fd_terminal\":{},\"active_store_connections\":{},\"owned_temp_journal_attempt_residue\":{},\"current_rss_bytes\":{},\"process_peak_rss_bytes\":{},\"maximum_rss_bytes\":{}}},\"store_database_bytes_max\":{},\"maximum_user_regular_file_bytes\":{FILE_BYTES},\"master_unchanged\":{master_unchanged}}}",
            if failed { "FAIL" } else { "PASS" },
            error
                .as_deref()
                .map(|value| format!("\"{}\"", json_escape(value)))
                .unwrap_or_else(|| "null".to_owned()),
            terminal.observed,
            terminal
                .observation_error
                .as_deref()
                .map(|value| format!("\"{}\"", json_escape(value)))
                .unwrap_or_else(|| "null".to_owned()),
            option_u64_json(campaign.data.last_q_terminal_bytes),
            observed_u64_json(terminal.observed, terminal.fd_baseline),
            observed_u64_json(terminal.observed, terminal.fd_terminal),
            observed_u64_json(terminal.observed, terminal.open_store_connections),
            observed_u64_json(terminal.observed, terminal.attempt_residue),
            observed_u64_json(terminal.observed, terminal.current_rss_bytes),
            observed_u64_json(terminal.observed, terminal.maximum_rss_bytes),
            observed_u64_json(terminal.observed, terminal.maximum_rss_bytes),
            option_u64_json(campaign.data.store_database_bytes_max),
        ),
        resources,
    )?;
    Ok(error)
}

fn append_failure_a16(
    run: &Path,
    started: Instant,
    data: &mut CampaignData,
    fd_baseline: Option<u64>,
) -> EvalResult<TerminalResources> {
    let terminal = match fd_baseline {
        Some(fd_baseline) => {
            terminal_resources(fd_baseline).unwrap_or_else(|error| TerminalResources {
                observation_error: Some(error),
                ..TerminalResources::default()
            })
        }
        None => TerminalResources {
            observation_error: Some("FD baseline unavailable".to_owned()),
            ..TerminalResources::default()
        },
    };
    let rows = OpenOptions::new()
        .append(true)
        .open(run.join("rows.jsonl"))
        .map_err(io_error)?;
    let mut campaign = Campaign {
        run,
        started,
        rows,
        data,
    };
    let _ = append_a16(&mut campaign, &terminal, false)?;
    campaign.rows.sync_all().map_err(io_error)?;
    Ok(terminal)
}

fn process_resources(operation: &str) -> EvalResult<ProcessResources> {
    Ok(ProcessResources {
        operation: operation.to_owned(),
        observed: true,
        current_rss_bytes: current_rss_bytes()?,
        process_peak_rss_bytes: maximum_rss_bytes()?,
    })
}

fn process_resources_json(value: &ProcessResources) -> String {
    let crossed = if value.observed {
        (value.process_peak_rss_bytes > 67_108_864).to_string()
    } else {
        "\"Unavailable\"".to_owned()
    };
    format!(
        "{{\"operation\":\"{}\",\"observed\":{},\"current_rss_bytes\":{},\"process_peak_rss_bytes\":{},\"crossed_64_mib\":{crossed}}}",
        json_escape(&value.operation),
        value.observed,
        observed_u64_json(value.observed, value.current_rss_bytes),
        observed_u64_json(value.observed, value.process_peak_rss_bytes),
    )
}

fn summary_json(
    status: &str,
    error: Option<&str>,
    data: &CampaignData,
    wall: u128,
    start_master: &str,
    final_master: &str,
    terminal: &TerminalResources,
) -> EvalResult<String> {
    if status == "PASS" {
        validate_metric_populations(data)?;
    }
    let disposition = if status == "PASS" {
        performance_disposition(data, wall)?
    } else {
        status.to_owned()
    };
    let metrics = data
        .metrics
        .iter()
        .map(|(name, observations)| {
            let statistics = statistics(observations)?;
            Ok(format!(
                "\"{}\":{}",
                json_escape(name),
                statistics_json(
                    name,
                    &statistics,
                    data.bytes_per_observation.get(name).copied()
                )
            ))
        })
        .collect::<EvalResult<Vec<_>>>()?
        .join(",");
    let targets = target_json(data, wall)?;
    let process_resources = process_resource_summary_json(data);
    let roots = data
        .output_roots
        .iter()
        .map(|(name, root)| format!("\"{}\":\"{}\"", json_escape(name), root))
        .collect::<Vec<_>>()
        .join(",");
    let phase_sum = data
        .reset_wall_ns
        .checked_add(data.open_wall_ns)
        .and_then(|value| value.checked_add(data.managed_prepare_wall_ns))
        .and_then(|value| value.checked_add(data.operation_wall_ns))
        .and_then(|value| value.checked_add(data.postcheck_wall_ns))
        .and_then(|value| value.checked_add(data.cleanup_wall_ns))
        .and_then(|value| value.checked_add(data.artifact_wall_ns))
        .ok_or_else(|| "campaign phase sum overflow".to_owned())?;
    if status == "PASS" && phase_sum > wall {
        return Err("campaign phase attribution exceeds complete wall".to_owned());
    }
    let timer_residual = wall.saturating_sub(phase_sum);
    let equation_sum = phase_sum.saturating_add(timer_residual);
    Ok(format!(
        concat!(
            "{{\"schema\":\"layerfs-stage1-summary-v2\",\"status\":\"{}\",\"error\":{},",
            "\"campaign_complete_wall_ns\":{},\"campaign_equation\":{{",
            "\"reset_ns\":{},\"open_ns\":{},\"managed_prepare_ns\":{},",
            "\"operation_ns\":{},\"postcheck_ns\":{},\"cleanup_ns\":{},",
            "\"artifact_ns\":{},\"timer_residual_ns\":{},\"sum_ns\":{},",
            "\"closed\":{}}},",
            "\"resets\":{},\"statistics\":{{{}}},\"targets\":{},",
            "\"process_resources\":{},",
            "\"output_roots\":{{{}}},\"master_start_blake3\":\"{}\",",
            "\"master_final_blake3\":\"{}\",",
            "\"terminal_receipt_rewrites_outside_accounted_wall\":[\"summary.json\",\"summary.md\",\"campaign-time.txt\"],",
            "\"maximum_user_regular_file_bytes\":{},\"store_database_bytes_max\":{},",
            "\"terminal\":{{\"fd_baseline\":{},\"fd_terminal\":{},",
            "\"attempt_residue\":{},\"active_store_connections\":{},",
            "\"current_rss_bytes\":{},\"process_peak_rss_bytes\":{},",
            "\"maximum_rss_bytes\":{},",
            "\"operation_q_bytes\":{}}}}}\n"
        ),
        json_escape(&disposition),
        error
            .map(|value| format!("\"{}\"", json_escape(value)))
            .unwrap_or_else(|| "null".to_owned()),
        wall,
        data.reset_wall_ns,
        data.open_wall_ns,
        data.managed_prepare_wall_ns,
        data.operation_wall_ns,
        data.postcheck_wall_ns,
        data.cleanup_wall_ns,
        data.artifact_wall_ns,
        timer_residual,
        equation_sum,
        equation_sum == wall,
        data.reset_count,
        metrics,
        targets,
        process_resources,
        roots,
        start_master,
        final_master,
        FILE_BYTES,
        option_u64_json(data.store_database_bytes_max),
        observed_u64_json(terminal.observed, terminal.fd_baseline),
        observed_u64_json(terminal.observed, terminal.fd_terminal),
        observed_u64_json(terminal.observed, terminal.attempt_residue),
        observed_u64_json(terminal.observed, terminal.open_store_connections),
        observed_u64_json(terminal.observed, terminal.current_rss_bytes),
        observed_u64_json(terminal.observed, terminal.maximum_rss_bytes),
        observed_u64_json(terminal.observed, terminal.maximum_rss_bytes),
        option_u64_json(data.last_q_terminal_bytes),
    ))
}

fn process_resource_summary_json(data: &CampaignData) -> String {
    let observations = data
        .process_resources
        .iter()
        .enumerate()
        .map(|(sequence, value)| {
            let crossed = if value.observed {
                (value.process_peak_rss_bytes > 67_108_864).to_string()
            } else {
                "\"Unavailable\"".to_owned()
            };
            format!(
                "{{\"sequence\":{sequence},\"operation\":\"{}\",\"observed\":{},\"current_rss_bytes\":{},\"process_peak_rss_bytes\":{},\"crossed_64_mib\":{crossed}}}",
                json_escape(&value.operation),
                value.observed,
                observed_u64_json(value.observed, value.current_rss_bytes),
                observed_u64_json(value.observed, value.process_peak_rss_bytes),
            )
        })
        .collect::<Vec<_>>();
    let first_crossing = data
        .process_resources
        .iter()
        .enumerate()
        .find(|(_, value)| value.observed && value.process_peak_rss_bytes > 67_108_864)
        .map_or_else(
            || "null".to_owned(),
            |(sequence, value)| {
                format!(
                    "{{\"sequence\":{sequence},\"operation\":\"{}\",\"current_rss_bytes\":{},\"process_peak_rss_bytes\":{}}}",
                    json_escape(&value.operation),
                    value.current_rss_bytes,
                    value.process_peak_rss_bytes,
                )
            },
        );
    format!(
        "{{\"observations\":[{}],\"first_64_mib_crossing\":{first_crossing}}}",
        observations.join(",")
    )
}

fn summary_markdown(data: &CampaignData, wall: u128) -> EvalResult<String> {
    validate_metric_populations(data)?;
    let disposition = performance_disposition(data, wall)?;
    let mut output = format!(
        "# LayerFS Stage One Part 1\n\nDisposition: {disposition}.\n\n- Complete wall: {wall} ns\n- Store resets: {} / {RESET_COUNT}\n- Maximum user file: {FILE_BYTES} bytes\n- Store database maximum (separate authority): {} bytes\n\n| Metric | n | min ns | p50 ns | p95 ns | max ns | throughput MiB/s | target |\n|---|---:|---:|---:|---:|---:|---:|---|\n",
        data.reset_count,
        data.store_database_bytes_max
            .map_or_else(|| "Unavailable".to_owned(), |value| value.to_string())
    );
    for (name, observations) in &data.metrics {
        let stats = statistics(observations)?;
        let throughput = data.bytes_per_observation.get(name).map_or_else(
            || "N/A".to_owned(),
            |bytes| format!("{:.3}", throughput_mib_s(*bytes, stats.p50)),
        );
        output.push_str(&format!(
            "| {name} | {} | {} | {} | {} | {} | {throughput} | {} |\n",
            stats.sorted.len(),
            stats.minimum,
            stats.p50,
            stats.p95,
            stats.maximum,
            target_label(name, &stats, data.bytes_per_observation.get(name).copied()),
        ));
    }
    Ok(output)
}

fn statistics(observations: &[u128]) -> EvalResult<Statistics> {
    if observations.is_empty() {
        return Err("statistics population is empty".to_owned());
    }
    let mut sorted = observations.to_vec();
    sorted.sort_unstable();
    let len = sorted.len();
    let p50 = if len % 2 == 1 {
        sorted[len / 2]
    } else {
        sorted[len / 2 - 1]
            .checked_add(sorted[len / 2])
            .ok_or_else(|| "p50 overflow".to_owned())?
            / 2
    };
    let p95_rank = (95 * len).div_ceil(100).max(1);
    let minimum = sorted[0];
    let maximum = sorted[len - 1];
    let p95 = sorted[p95_rank - 1];
    let operation_wall = sorted.iter().try_fold(0_u128, |total, value| {
        total
            .checked_add(*value)
            .ok_or_else(|| "statistics wall overflow".to_owned())
    })?;
    Ok(Statistics {
        sorted,
        minimum,
        maximum,
        range: maximum - minimum,
        p50,
        p95,
        operation_wall,
    })
}

fn statistics_json(name: &str, value: &Statistics, bytes: Option<u64>) -> String {
    let raw = json_u128_array(&value.sorted);
    let throughput = bytes.map_or_else(
        || "null".to_owned(),
        |bytes| format!("{:.6}", throughput_mib_s(bytes, value.p50)),
    );
    let aggregate = bytes.map_or_else(
        || "null".to_owned(),
        |bytes| {
            let total = u128::from(bytes) * value.sorted.len() as u128;
            format!("{:.6}", throughput_mib_s_u128(total, value.operation_wall))
        },
    );
    format!(
        "{{\"raw_sorted_ns\":{raw},\"minimum_ns\":{},\"maximum_ns\":{},\"range_ns\":{},\"p50_ns\":{},\"p95_ns\":{},\"operation_population_wall_ns\":{},\"bytes_per_observation\":{},\"p50_throughput_mib_s\":{throughput},\"aggregate_throughput_mib_s\":{aggregate},\"target\":{}}}",
        value.minimum,
        value.maximum,
        value.range,
        value.p50,
        value.p95,
        value.operation_wall,
        bytes.map_or_else(|| "null".to_owned(), |value| value.to_string()),
        target_json_for_metric(name, value, bytes),
    )
}

fn throughput_mib_s(bytes: u64, nanoseconds: u128) -> f64 {
    throughput_mib_s_u128(u128::from(bytes), nanoseconds)
}

fn throughput_mib_s_u128(bytes: u128, nanoseconds: u128) -> f64 {
    if nanoseconds == 0 {
        return f64::MAX;
    }
    bytes as f64 / MIB as f64 / (nanoseconds as f64 / 1_000_000_000.0)
}

fn target_json_for_metric(name: &str, stats: &Statistics, bytes: Option<u64>) -> String {
    format!("\"{}\"", json_escape(&target_label(name, stats, bytes)))
}

fn target_label(name: &str, stats: &Statistics, bytes: Option<u64>) -> String {
    let throughput = bytes.map(|bytes| throughput_mib_s(bytes, stats.p50));
    let (description, pass) = match name {
        "A01" => (
            ">=250 MiB/s",
            throughput.is_some_and(|value| value >= 250.0),
        ),
        "A02" => (
            "p50<=0.5ms and p95<=1.0ms",
            stats.p50 <= 500_000 && stats.p95 <= 1_000_000,
        ),
        "A03a" | "A03b" => (
            ">=150 MiB/s",
            throughput.is_some_and(|value| value >= 150.0),
        ),
        "A04/logical" => ("p50<=15ms", stats.p50 <= 15_000_000),
        "A04/native-edit-plus-checkpoint" => ("p50<=20ms", stats.p50 <= 20_000_000),
        "A09" => (
            ">=200 MiB/s",
            throughput.is_some_and(|value| value >= 200.0),
        ),
        "A10" => (
            ">=150 MiB/s",
            throughput.is_some_and(|value| value >= 150.0),
        ),
        "A11" => ("p50<=5ms", stats.p50 <= 5_000_000),
        "A12" => ("p50<=25ms", stats.p50 <= 25_000_000),
        "A13" => ("p50<=4ms", stats.p50 <= 4_000_000),
        _ => return "REPORT_ONLY".to_owned(),
    };
    format!("{} ({description})", if pass { "PASS" } else { "REVISE" })
}

fn target_json(data: &CampaignData, wall: u128) -> EvalResult<String> {
    let mut values = Vec::new();
    for (name, observations) in &data.metrics {
        let stats = statistics(observations)?;
        let label = target_label(name, &stats, data.bytes_per_observation.get(name).copied());
        if label != "REPORT_ONLY" {
            values.push(format!("\"{}\":\"{}\"", json_escape(name), label));
        }
    }
    values.push(format!(
        "\"complete_campaign\":\"{} (preferred<60s hard<=120s)\"",
        if wall <= CAMPAIGN_LIMIT_NS {
            if wall < 60_000_000_000 {
                "PASS"
            } else {
                "PASS_HARD_REVISE_PREFERRED"
            }
        } else {
            "FAIL_HARD"
        }
    ));
    Ok(format!("{{{}}}", values.join(",")))
}

fn performance_disposition(data: &CampaignData, wall: u128) -> EvalResult<String> {
    if wall >= 60_000_000_000 {
        return Ok("REVISE".to_owned());
    }
    for (name, observations) in &data.metrics {
        let stats = statistics(observations)?;
        if target_label(name, &stats, data.bytes_per_observation.get(name).copied())
            .starts_with("REVISE")
        {
            return Ok("REVISE".to_owned());
        }
    }
    Ok("PASS".to_owned())
}

fn validate_metric_populations(data: &CampaignData) -> EvalResult<()> {
    let mut expected = BTreeMap::from([
        ("A01".to_owned(), 3_usize),
        ("A02".to_owned(), 300),
        ("A03a".to_owned(), 3),
        ("A03b".to_owned(), 3),
        ("A09".to_owned(), 3),
        ("A10".to_owned(), 3),
        ("A11".to_owned(), 3),
        ("A12".to_owned(), 3),
        ("A13".to_owned(), 11),
        ("A14/edit".to_owned(), 4),
        ("A15".to_owned(), 3),
        ("A17/checkpoint".to_owned(), 100),
        ("A17/edit-plus-checkpoint".to_owned(), 100),
    ]);
    for id in ["A04", "A05", "A06", "A07", "A08"] {
        expected.insert(format!("{id}/logical"), 3);
        expected.insert(format!("{id}/native-edit-plus-checkpoint"), 3);
    }
    for (name, count) in expected {
        let actual = data.metrics.get(&name).map_or(0, Vec::len);
        if actual != count {
            return Err(format!("metric population {name}: {actual} != {count}"));
        }
    }
    Ok(())
}

fn base<'a>(master: &'a Master, name: &str) -> EvalResult<&'a BaseManifest> {
    master
        .bases
        .get(name)
        .ok_or_else(|| format!("fixture base {name} missing"))
}

fn environment() -> EvalResult<Environment> {
    let git_commit = command_output("git", &["rev-parse", "HEAD"])?
        .trim()
        .to_owned();
    let (dirty_tree_blake3, source_tree_blake3, source_files) = source_fingerprints()?;
    let executable = std::env::current_exe().map_err(io_error)?;
    Ok(Environment {
        git_commit,
        dirty_tree_blake3,
        source_tree_blake3,
        source_file_count: source_files.len() as u64,
        source_files,
        cargo_lock_blake3: hash_file(&workspace_root().join("Cargo.lock"))?,
        executable_blake3: hash_file(&executable)?,
        build_profile: if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        },
        debug_assertions: cfg!(debug_assertions),
        uname: command_output("uname", &["-a"])?.trim().to_owned(),
        macos: command_output("sw_vers", &[]).unwrap_or_else(|_| "Unavailable".to_owned()),
        apfs_identity: assert_apfs(&fixture_root()).unwrap_or_else(|_| "Unavailable".to_owned()),
    })
}

pub(crate) fn preparation_source_context_json() -> EvalResult<String> {
    let value = environment()?;
    let source_files = value
        .source_files
        .iter()
        .map(|path| format!("\"{}\"", json_escape(path)))
        .collect::<Vec<_>>()
        .join(",");
    Ok(format!(
        concat!(
            "{{\"git_commit\":\"{}\",\"dirty_tree_blake3\":\"{}\",",
            "\"source_tree_blake3\":\"{}\",\"source_files\":[{}],",
            "\"cargo_lock_blake3\":\"{}\",\"executable_blake3\":\"{}\",",
            "\"build_profile\":\"{}\",\"debug_assertions\":{}}}"
        ),
        json_escape(&value.git_commit),
        value.dirty_tree_blake3,
        value.source_tree_blake3,
        source_files,
        value.cargo_lock_blake3,
        value.executable_blake3,
        value.build_profile,
        value.debug_assertions,
    ))
}

fn source_fingerprints() -> EvalResult<(String, String, Vec<String>)> {
    let diff = command_bytes("git", &["diff", "--binary", "HEAD"])?;
    let untracked = command_bytes("git", &["ls-files", "--others", "--exclude-standard", "-z"])?;
    let mut paths = untracked
        .split(|byte| *byte == 0)
        .filter(|path| !path.is_empty())
        .map(|path| String::from_utf8(path.to_vec()).map_err(display_error))
        .collect::<EvalResult<Vec<_>>>()?;
    paths.sort();
    let mut dirty = blake3::Hasher::new();
    dirty.update(&diff);
    for path in &paths {
        dirty.update(path.as_bytes());
        dirty.update(&[0]);
        let bytes = fs::read(workspace_root().join(path)).map_err(io_error)?;
        dirty.update(blake3::hash(&bytes).as_bytes());
    }

    let tracked = command_bytes(
        "git",
        &[
            "ls-files",
            "-co",
            "--exclude-standard",
            "-z",
            "--",
            "*.rs",
            "Cargo.toml",
            "Cargo.lock",
        ],
    )?;
    let mut source_paths = tracked
        .split(|byte| *byte == 0)
        .filter(|path| !path.is_empty())
        .map(|path| String::from_utf8(path.to_vec()).map_err(display_error))
        .collect::<EvalResult<Vec<_>>>()?;
    source_paths.sort();
    source_paths.dedup();
    let mut source = blake3::Hasher::new();
    for path in &source_paths {
        source.update(path.as_bytes());
        source.update(&[0]);
        source.update(&fs::read(workspace_root().join(path)).map_err(io_error)?);
    }
    Ok((
        dirty.finalize().to_hex().to_string(),
        source.finalize().to_hex().to_string(),
        source_paths,
    ))
}

fn environment_json(value: &Environment) -> String {
    let source_files = value
        .source_files
        .iter()
        .map(|path| format!("\"{}\"", json_escape(path)))
        .collect::<Vec<_>>()
        .join(",");
    format!(
        concat!(
            "{{\"schema\":\"layerfs-stage1-environment-v2\",\"git_commit\":\"{}\",",
            "\"dirty_tree_blake3\":\"{}\",\"source_tree_blake3\":\"{}\",",
            "\"source_file_count\":{},\"source_files\":[{}],\"cargo_lock_blake3\":\"{}\",",
            "\"executable_blake3\":\"{}\",\"build_profile\":\"{}\",",
            "\"debug_assertions\":{},\"maximum_user_regular_file_bytes\":{},",
            "\"largest_product_buffer_bytes\":{},\"uname\":\"{}\",\"macos\":\"{}\",",
            "\"apfs_identity\":\"{}\",",
            "\"build_command\":\"cargo build -p layerfs-eval --release\",",
            "\"command\":\"layerfs-eval stage1 run single-file <run-directory>\"}}\n"
        ),
        json_escape(&value.git_commit),
        value.dirty_tree_blake3,
        value.source_tree_blake3,
        value.source_file_count,
        source_files,
        value.cargo_lock_blake3,
        value.executable_blake3,
        value.build_profile,
        value.debug_assertions,
        FILE_BYTES,
        BUFFER_BYTES,
        json_escape(&value.uname),
        json_escape(&value.macos),
        json_escape(&value.apfs_identity),
    )
}

fn durable_write(path: &Path, contents: &str) -> EvalResult<()> {
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(path)
        .map_err(io_error)?;
    file.write_all(contents.as_bytes()).map_err(io_error)?;
    file.sync_all().map_err(io_error)
}

fn durable_replace(path: &Path, contents: &str) -> EvalResult<()> {
    let parent = path
        .parent()
        .ok_or_else(|| "durable replacement has no parent".to_owned())?;
    let temporary = parent.join(format!(".summary-json-final-{}", std::process::id()));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .map_err(io_error)?;
    file.write_all(contents.as_bytes()).map_err(io_error)?;
    file.sync_all().map_err(io_error)?;
    fs::rename(&temporary, path).map_err(io_error)?;
    File::open(parent)
        .and_then(|directory| directory.sync_all())
        .map_err(io_error)
}

fn sync_rows(run: &Path) -> EvalResult<()> {
    match OpenOptions::new().read(true).open(run.join("rows.jsonl")) {
        Ok(rows) => rows.sync_all().map_err(io_error),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(io_error(error)),
    }
}

fn fd_count() -> EvalResult<u64> {
    let path = if Path::new("/dev/fd").is_dir() {
        Path::new("/dev/fd")
    } else {
        Path::new("/proc/self/fd")
    };
    Ok(fs::read_dir(path).map_err(io_error)?.count() as u64)
}

fn attempt_residue_count() -> EvalResult<u64> {
    let path = workspace_root().join("target/layerfs-stage1-attempts");
    match fs::read_dir(path) {
        Ok(entries) => Ok(entries.count() as u64),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(0),
        Err(error) => Err(io_error(error)),
    }
}

fn open_store_connection_count() -> EvalResult<u64> {
    let pid = std::process::id().to_string();
    let output = command_output("/usr/sbin/lsof", &["-Fn", "-p", &pid])?;
    Ok(output
        .lines()
        .filter(|line| {
            line.starts_with('n')
                && line.contains("generation-")
                && line.contains(".sqlite")
                && line.contains("layerfs-stage1")
        })
        .count() as u64)
}

fn current_rss_bytes() -> EvalResult<u64> {
    let pid = std::process::id().to_string();
    let output = command_output("/bin/ps", &["-o", "rss=", "-p", &pid])?;
    let kib = output.trim().parse::<u64>().map_err(display_error)?;
    kib.checked_mul(1_024)
        .ok_or_else(|| "RSS byte conversion overflow".to_owned())
}

#[cfg(target_os = "macos")]
fn maximum_rss_bytes() -> EvalResult<u64> {
    use std::ffi::c_int;

    #[repr(C)]
    #[derive(Default)]
    struct TimeVal {
        seconds: i64,
        microseconds: i64,
    }

    #[repr(C)]
    #[derive(Default)]
    struct RUsage {
        user: TimeVal,
        system: TimeVal,
        maximum_resident_set_bytes: i64,
        remaining: [i64; 13],
    }

    unsafe extern "C" {
        fn getrusage(who: c_int, usage: *mut RUsage) -> c_int;
    }

    let mut usage = RUsage::default();
    // SAFETY: `usage` is a live Darwin-compatible rusage buffer for the call.
    if unsafe { getrusage(0, &mut usage) } != 0 || usage.maximum_resident_set_bytes < 0 {
        return Err(std::io::Error::last_os_error().to_string());
    }
    Ok(usage.maximum_resident_set_bytes as u64)
}

#[cfg(not(target_os = "macos"))]
fn maximum_rss_bytes() -> EvalResult<u64> {
    current_rss_bytes()
}

fn command_output(program: &str, arguments: &[&str]) -> EvalResult<String> {
    String::from_utf8(command_bytes(program, arguments)?).map_err(display_error)
}

fn command_bytes(program: &str, arguments: &[&str]) -> EvalResult<Vec<u8>> {
    let output = Command::new(program)
        .args(arguments)
        .current_dir(workspace_root())
        .output()
        .map_err(io_error)?;
    if !output.status.success() {
        return Err(format!(
            "{program} {} exited {}",
            arguments.join(" "),
            output.status
        ));
    }
    Ok(output.stdout)
}

fn json_escape(value: &str) -> String {
    value
        .chars()
        .flat_map(|character| match character {
            '"' => "\\\"".chars().collect::<Vec<_>>(),
            '\\' => "\\\\".chars().collect(),
            '\n' => "\\n".chars().collect(),
            '\r' => "\\r".chars().collect(),
            '\t' => "\\t".chars().collect(),
            character if character.is_control() => "?".chars().collect(),
            character => vec![character],
        })
        .collect()
}

fn json_string(json: &str, key: &str) -> EvalResult<String> {
    let needle = format!("\"{key}\":\"");
    let start = json
        .find(&needle)
        .ok_or_else(|| format!("missing JSON string {key}"))?
        + needle.len();
    let end = json[start..]
        .find('"')
        .ok_or_else(|| format!("unterminated JSON string {key}"))?
        + start;
    Ok(json[start..end].to_owned())
}

fn json_u128(json: &str, key: &str) -> EvalResult<u128> {
    let needle = format!("\"{key}\":");
    let start = json
        .find(&needle)
        .ok_or_else(|| format!("missing JSON integer {key}"))?
        + needle.len();
    let end = json[start..]
        .find(|character: char| !character.is_ascii_digit())
        .map_or(json.len(), |offset| start + offset);
    json[start..end].parse::<u128>().map_err(display_error)
}

fn json_bool(json: &str, key: &str) -> EvalResult<bool> {
    let needle = format!("\"{key}\":");
    let start = json
        .find(&needle)
        .ok_or_else(|| format!("missing JSON boolean {key}"))?
        + needle.len();
    if json[start..].starts_with("true") {
        Ok(true)
    } else if json[start..].starts_with("false") {
        Ok(false)
    } else {
        Err(format!("invalid JSON boolean {key}"))
    }
}

fn json_u128_array(values: &[u128]) -> String {
    format!(
        "[{}]",
        values
            .iter()
            .map(u128::to_string)
            .collect::<Vec<_>>()
            .join(",")
    )
}

fn io_error(error: std::io::Error) -> String {
    error.to_string()
}

fn display_error(error: impl std::fmt::Display) -> String {
    error.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(target_os = "macos")]
    fn valid_json(document: &str) {
        use std::process::Stdio;

        let mut child = Command::new("/usr/bin/plutil")
            .args(["-convert", "json", "-o", "/dev/null", "-"])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(document.as_bytes())
            .unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            document,
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[test]
    fn frozen_statistics_use_one_based_order_rules() {
        let heavy = statistics(&[30, 10, 20]).unwrap();
        assert_eq!((heavy.p50, heavy.p95), (20, 30));

        let reopen = statistics(&(1..=11).collect::<Vec<_>>()).unwrap();
        assert_eq!((reopen.p50, reopen.p95), (6, 11));

        let random = statistics(&(1..=300).collect::<Vec<_>>()).unwrap();
        assert_eq!((random.p50, random.p95), (150, 285));
    }

    #[test]
    fn frozen_reset_and_edit_populations_are_exact() {
        assert_eq!(3 + 3 + 3 + 3 + 30 + 3 + 3 + 1 + 1 + 3 + 1, RESET_COUNT);
        let cases = edit_cases();
        assert_eq!(cases.len(), 5);
        assert_eq!(edit_result_len(&cases[1]).unwrap(), FILE_BYTES);
        assert_eq!(edit_result_len(&cases[3]).unwrap(), FILE_BYTES);
        assert!(cases
            .iter()
            .all(|case| edit_result_len(case).unwrap() <= FILE_BYTES));
    }

    #[test]
    fn content_locality_excludes_observed_metadata_ropes() {
        let mut counters = OperationDiagnostics::default();
        counters.rope.cdc_bytes_scanned = 4_112;
        counters.rope.payload_bytes_written = 4_112;
        counters.metadata_rope.cdc_bytes_scanned = 16;
        counters.metadata_rope.payload_bytes_written = 16;
        assert_eq!(content_rope(&counters).unwrap(), (4_096, 0, 4_096));
    }

    #[test]
    fn random_ranges_are_globally_non_overlapping() {
        let blocks = FILE_BYTES / RANDOM_RANGE_BYTES;
        let offsets = (0..300_u64)
            .map(|index| ((index * 521 + 0x51) % blocks) * RANDOM_RANGE_BYTES)
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(offsets.len(), 300);
    }

    #[test]
    fn readiness_artifacts_are_append_only() {
        let root = std::env::temp_dir().join(format!(
            "layerfs-readiness-artifacts-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&root).unwrap();
        let first = append_only_readiness_artifact(&root, "failure", "first\n").unwrap();
        let second = append_only_readiness_artifact(&root, "failure", "second\n").unwrap();
        assert_ne!(first, second);
        assert_eq!(fs::read_to_string(first).unwrap(), "first\n");
        assert_eq!(fs::read_to_string(second).unwrap(), "second\n");
        let canonical = root.join("summary.json");
        fs::write(&canonical, "fail\n").unwrap();
        durable_replace(&canonical, "pass\n").unwrap();
        assert_eq!(fs::read_to_string(canonical).unwrap(), "pass\n");
        assert_eq!(fs::read_dir(&root).unwrap().count(), 3);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn failing_a16_is_appended_before_the_gate_error() {
        let root = std::env::temp_dir().join(format!(
            "layerfs-a16-artifact-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&root).unwrap();
        let rows_path = root.join("rows.jsonl");
        fs::write(&rows_path, "").unwrap();
        let rows = OpenOptions::new().append(true).open(&rows_path).unwrap();
        let mut data = CampaignData {
            last_q_terminal_bytes: Some(0),
            ..CampaignData::default()
        };
        let mut campaign = Campaign {
            run: &root,
            started: Instant::now(),
            rows,
            data: &mut data,
        };
        let terminal = TerminalResources {
            observed: true,
            fd_baseline: 5,
            fd_terminal: 5,
            current_rss_bytes: 65 * MIB,
            maximum_rss_bytes: 65 * MIB,
            ..TerminalResources::default()
        };
        let error = append_a16(&mut campaign, &terminal, true).unwrap().unwrap();
        campaign.rows.sync_all().unwrap();
        drop(campaign);

        let row = fs::read_to_string(&rows_path).unwrap();
        let lines = row.lines().collect::<Vec<_>>();
        assert_eq!(lines.len(), 1);
        #[cfg(target_os = "macos")]
        valid_json(lines[0]);
        assert!(error.contains("peak RSS"));
        assert!(row.contains("\"id\":\"A16\""));
        assert!(row.contains("\"gate_status\":\"FAIL\""));
        assert!(row.contains("\"operation_q_bytes\":0"));
        assert!(row.contains("\"process_peak_rss_bytes\":68157440"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn earlier_failure_appends_an_unavailable_a16_row() {
        let root = std::env::temp_dir().join(format!(
            "layerfs-a16-early-failure-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&root).unwrap();
        fs::write(root.join("rows.jsonl"), "").unwrap();
        let mut data = CampaignData::default();
        let terminal = append_failure_a16(&root, Instant::now(), &mut data, None).unwrap();

        assert!(!terminal.observed);
        let row = fs::read_to_string(root.join("rows.jsonl")).unwrap();
        let lines = row.lines().collect::<Vec<_>>();
        assert_eq!(lines.len(), 1);
        #[cfg(target_os = "macos")]
        valid_json(lines[0]);
        assert!(row.contains("\"id\":\"A16\""));
        assert!(row.contains("\"gate_status\":\"FAIL\""));
        assert!(row.contains("\"observed\":false"));
        assert!(row.contains("\"fd_baseline\":\"Unavailable\""));
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(target_os = "macos")]
    #[test]
    #[ignore = "requires the prepared 100 MiB APFS fixture"]
    fn full_import_reports_the_phase_that_owns_peak_rss() {
        struct RssRead<R> {
            inner: R,
            bytes: u64,
            next_sample: u64,
            samples: std::rc::Rc<std::cell::RefCell<Vec<(u64, u64)>>>,
        }

        impl<R: Read> Read for RssRead<R> {
            fn read(&mut self, output: &mut [u8]) -> std::io::Result<usize> {
                let read = self.inner.read(output)?;
                self.bytes += read as u64;
                if self.bytes >= self.next_sample {
                    self.samples
                        .borrow_mut()
                        .push((self.bytes, maximum_rss_bytes().unwrap()));
                    self.next_sample += 10 * MIB;
                }
                Ok(read)
            }
        }

        let master = read_master(&fixture_root()).unwrap();
        let expected = base(&master, "import-genesis").unwrap();
        let baseline = maximum_rss_bytes().unwrap();
        let attempt = Attempt::create("import-genesis", expected).unwrap();
        let after_reset = maximum_rss_bytes().unwrap();
        let opened = attempt
            .open(expected, IntegrityMode::TrustedLocalDev)
            .unwrap();
        let after_open = maximum_rss_bytes().unwrap();
        let samples = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let input = BoundedRead(RssRead {
            inner: File::open(input_path(false)).unwrap(),
            bytes: 0,
            next_sample: 10 * MIB,
            samples: samples.clone(),
        });
        let (state, _) = opened
            .fs
            .replace_file_observed(&expected_ref(expected), FILE_PATH, input)
            .unwrap();
        let after_replace = maximum_rss_bytes().unwrap();
        assert_eq!(opened.fs.current_head("main").unwrap(), state);
        let after_head = maximum_rss_bytes().unwrap();
        let (bytes, digest, _) = canonical_digest(&opened.fs, state.root).unwrap();
        let after_digest = maximum_rss_bytes().unwrap();
        eprintln!(
            "peak_rss baseline={baseline} reset={after_reset} open={after_open} stream={:?} replace={after_replace} head={after_head} digest={after_digest}",
            samples.borrow()
        );
        assert_eq!((bytes, digest), (FILE_BYTES, master.raw_digest.clone()));
        assert!(
            after_digest <= 67_108_864,
            "full import exceeded 64 MiB RSS"
        );
        drop(opened);
        attempt.cleanup().unwrap();
    }

    #[cfg(target_os = "macos")]
    #[test]
    #[ignore = "requires the prepared 100 MiB APFS fixture"]
    fn random_ranges_reuse_the_exact_file_read_plan() {
        let master = read_master(&fixture_root()).unwrap();
        let expected = base(&master, "read-reconstruct").unwrap();
        let blocks = FILE_BYTES / RANDOM_RANGE_BYTES;
        let mut observations = Vec::with_capacity(300);
        let mut statements = 0;
        let mut fetched_rows = 0;
        for batch in 0..3_u64 {
            let attempt = Attempt::create("read-reconstruct", expected).unwrap();
            let opened = attempt
                .open(expected, IntegrityMode::TrustedLocalDev)
                .unwrap();
            let before = opened.fs.counter_snapshot().unwrap();
            for within in 0..100_u64 {
                let offset = (((batch * 100 + within) * 521 + 0x51) % blocks) * RANDOM_RANGE_BYTES;
                let mut output = Vec::with_capacity(RANDOM_RANGE_BYTES as usize);
                let started = Instant::now();
                opened
                    .fs
                    .read_range(
                        expected.root,
                        FILE_PATH,
                        offset..offset + RANDOM_RANGE_BYTES,
                        &mut output,
                    )
                    .unwrap();
                observations.push(started.elapsed().as_nanos());
                assert_eq!(
                    output,
                    expected_bytes(offset, RANDOM_RANGE_BYTES as usize).unwrap()
                );
            }
            let after = opened.fs.diagnostics().unwrap();
            statements += after.statements - before.statements;
            fetched_rows += after.fetched_rows - before.fetched_rows;
            drop(opened);
            attempt.cleanup().unwrap();
        }
        let observed = statistics(&observations).unwrap();
        eprintln!(
            "A02 focused p50_ns={} p95_ns={} statements={} fetched_rows={}",
            observed.p50, observed.p95, statements, fetched_rows
        );
        assert_eq!(statements, 643);
        assert_eq!(fetched_rows, 1632);
    }

    #[test]
    fn failure_summary_retains_partial_statistics_targets_and_resources() {
        let mut data = CampaignData {
            reset_count: 1,
            reset_wall_ns: 10,
            operation_wall_ns: 30,
            artifact_wall_ns: 5,
            last_q_terminal_bytes: Some(0),
            ..CampaignData::default()
        };
        data.metrics.insert("A01".to_owned(), vec![30]);
        data.bytes_per_observation
            .insert("A01".to_owned(), FILE_BYTES);
        data.process_resources.push(ProcessResources {
            operation: "campaign-baseline".to_owned(),
            observed: true,
            current_rss_bytes: 50 * MIB,
            process_peak_rss_bytes: 50 * MIB,
        });
        data.process_resources.push(ProcessResources {
            operation: "A01".to_owned(),
            observed: true,
            current_rss_bytes: 60 * MIB,
            process_peak_rss_bytes: 65 * MIB,
        });
        let terminal = TerminalResources {
            observed: true,
            current_rss_bytes: 60 * MIB,
            maximum_rss_bytes: 65 * MIB,
            ..TerminalResources::default()
        };
        let summary = summary_json(
            "FAIL",
            Some("resource gate"),
            &data,
            100,
            "start",
            "final",
            &terminal,
        )
        .unwrap();

        assert!(summary.contains("\"schema\":\"layerfs-stage1-summary-v2\""));
        assert!(summary.contains("\"status\":\"FAIL\""));
        assert!(summary.contains("\"statistics\":{\"A01\":"));
        assert!(summary.contains("\"targets\":{\"A01\":"));
        assert!(summary.contains("\"campaign_equation\":{"));
        assert!(summary.contains("\"first_64_mib_crossing\":{\"sequence\":1,\"operation\":\"A01\""));
    }

    #[test]
    fn sdk_q_and_native_call_observations_are_not_zeroed_by_serialization() {
        let mut observed = OperationDiagnostics {
            operation_q_current_bytes: 4 * MIB,
            operation_q_high_water_bytes: 4 * MIB,
            operation_q_terminal_bytes: 0,
            ..OperationDiagnostics::default()
        };
        observed.native.route = Some(NativeRoute::MaterializeStream);
        observed.native.temp_calls = 1;
        observed.native.sync_calls = 2;
        observed.native.replace_calls = 1;
        let json = counters_json(&observed);
        assert!(json.contains("\"operation_q_current_bytes\":4194304"));
        assert!(json.contains("\"operation_q_high_water_bytes\":4194304"));
        assert!(json.contains("\"temp_calls\":1"));
        assert!(json.contains("\"sync_calls\":2"));
        assert!(json.contains("\"replace_calls\":1"));
        assert!(json.contains("\"rematerializations\":0"));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn artifact_json_is_valid() {
        let environment = Environment {
            git_commit: "a".repeat(40),
            dirty_tree_blake3: "b".repeat(64),
            source_tree_blake3: "c".repeat(64),
            source_file_count: 1,
            source_files: vec!["tools/layerfs-eval/src/stage1.rs".to_owned()],
            cargo_lock_blake3: "d".repeat(64),
            executable_blake3: "e".repeat(64),
            build_profile: "release",
            debug_assertions: false,
            uname: "Darwin".to_owned(),
            macos: "macOS".to_owned(),
            apfs_identity: "APFS".to_owned(),
        };
        let mut diagnostics = Diagnostics {
            page_size: STORE_PAGE_SIZE,
            cache_pages: STORE_CACHE_PAGES,
            cache_spill_pages: STORE_CACHE_SPILL_PAGES,
            ..Diagnostics::default()
        };
        let store_sqlite_profile = validate_store_sqlite_profile(&diagnostics).unwrap();
        diagnostics.cache_pages -= 1;
        assert!(validate_store_sqlite_profile(&diagnostics).is_err());
        let readiness = Readiness {
            environment: environment.clone(),
            master_digest: "master".to_owned(),
            reset_observations_ns: vec![1, 1, 1],
            reset_upper_ns: 1,
            forecast_reset_wall_ns: 54,
            forecast_campaign_wall_ns: 55,
            apfs_identity: "APFS".to_owned(),
            store_database_bytes: BTreeMap::new(),
            store_sqlite_profile,
        };
        let readiness = readiness_json(&readiness);
        valid_json(&readiness);
        assert_eq!(json_u128(&readiness, "page_size").unwrap(), 4_096);
        assert_eq!(json_u128(&readiness, "cache_pages").unwrap(), 1_280);
        assert_eq!(json_u128(&readiness, "cache_spill_pages").unwrap(), 1_280);
        valid_json(&environment_json(&environment));
        valid_json(&schedule_json(true));
        valid_json(&incomplete_summary_json(0));
        valid_json(locality_evidence_json());

        let mut data = CampaignData::default();
        let populations = [
            ("A01", 3),
            ("A02", 300),
            ("A03a", 3),
            ("A03b", 3),
            ("A09", 3),
            ("A10", 3),
            ("A11", 3),
            ("A12", 3),
            ("A13", 11),
            ("A14/edit", 4),
            ("A15", 3),
            ("A17/checkpoint", 100),
            ("A17/edit-plus-checkpoint", 100),
        ];
        for (name, count) in populations {
            data.metrics.insert(name.to_owned(), vec![1; count]);
        }
        for id in ["A04", "A05", "A06", "A07", "A08"] {
            data.metrics.insert(format!("{id}/logical"), vec![1; 3]);
            data.metrics
                .insert(format!("{id}/native-edit-plus-checkpoint"), vec![1; 3]);
        }
        let terminal = TerminalResources::default();
        valid_json(
            &summary_json("PASS", None, &data, 1_000, "master", "master", &terminal).unwrap(),
        );
        valid_json(&format!(
            "{{\"schema\":\"layerfs-stage1-row-v1\",\"operation_counters\":{},\"engine_delta\":{},\"clone\":{}}}",
            counters_json(&OperationDiagnostics::default()),
            engine_json(&EngineDelta::default()),
            clone_json(&CloneReceipt {
                wall_ns: 1,
                clone_wall_ns: 1,
                source_logical_bytes: 1,
                destination_logical_bytes: 1,
                source_allocated_bytes: 1,
                destination_allocated_bytes: 1,
                distinct_regular_inodes: 1,
                clone_id: 1,
            })
        ));
    }
}
