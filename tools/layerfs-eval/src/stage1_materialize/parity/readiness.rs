use super::super::contract::EvalResult;
use super::super::error::{display_error, io_error};
use super::super::evidence::digest::{digest_file, sha256_file};
use super::super::prepare::{durable_write, json_escape, unix_ns};
use std::fs;
use std::path::Path;
use std::process::Command;
use std::time::Instant;

pub fn parity_readiness(
    historical: &Path,
    instrumented: &Path,
    store: &Path,
    source: &Path,
    receipt: &Path,
) -> EvalResult<()> {
    if receipt.exists() {
        return Err(format!(
            "refusing to replace readiness {}",
            receipt.display()
        ));
    }
    let historical = historical.canonicalize().map_err(io_error)?;
    let instrumented = instrumented.canonicalize().map_err(io_error)?;
    let store = store.canonicalize().map_err(io_error)?;
    let source = source.canonicalize().map_err(io_error)?;
    if fs::metadata(&source).map_err(io_error)?.len() != 24 * 1024 * 1024 {
        return Err("parity source is not exactly 24 MiB".to_owned());
    }
    let source_digest = digest_file(&source)?;
    let historical_sha256 = sha256_file(&historical)?;
    let instrumented_sha256 = sha256_file(&instrumented)?;
    let parent = receipt
        .parent()
        .ok_or_else(|| "readiness receipt has no parent".to_owned())?;
    fs::create_dir_all(parent).map_err(io_error)?;
    let reset = parent.join(format!(
        ".stage1m-parity-readiness-reset-{}-{}",
        std::process::id(),
        unix_ns()?
    ));
    let started = Instant::now();
    let output = Command::new(&historical)
        .args(["stage1", "materialize", "parity-row"])
        .arg(&store)
        .arg(&source)
        .arg("24")
        .arg(&reset)
        .arg("readiness-historical")
        .output()
        .map_err(io_error)?;
    let reset_wall_ns = started.elapsed().as_nanos();
    if !output.status.success() {
        return Err(format!(
            "historical readiness reset failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    let stdout = String::from_utf8(output.stdout).map_err(display_error)?;
    let rows = stdout.lines().collect::<Vec<_>>();
    if rows.len() != 2
        || !rows[0].contains("\"row_kind\":\"warmup\"")
        || !rows[1].contains("\"row_kind\":\"measured\"")
        || rows.iter().any(|row| !row.contains("\"status\":\"PASS\""))
    {
        return Err("historical readiness reset returned invalid rows".to_owned());
    }
    let forecast_wall_ns = reset_wall_ns
        .checked_mul(8)
        .ok_or_else(|| "parity forecast overflow".to_owned())?;
    if forecast_wall_ns >= 10_000_000_000 {
        return Err(format!(
            "parity forecast {forecast_wall_ns}ns reaches the 10s hard wall"
        ));
    }
    let schedule = parity_schedule_json();
    let json = format!(
        concat!(
            "{{\"schema\":\"layerfs-stage1m-parity-readiness-v1\",",
            "\"status\":\"PASS\",\"measured_rows_started\":false,",
            "\"historical_path\":\"{}\",\"historical_sha256\":\"{}\",",
            "\"instrumented_path\":\"{}\",\"instrumented_sha256\":\"{}\",",
            "\"store\":\"{}\",\"source\":\"{}\",\"source_blake3\":\"{}\",",
            "\"schedule_blake3\":\"{}\",\"reset_wall_ns\":{},",
            "\"forecast_wall_ns\":{},\"hard_wall_ns\":10000000000,",
            "\"expected_warmups\":8,\"expected_measured\":8}}\n"
        ),
        json_escape(&historical.display().to_string()),
        historical_sha256,
        json_escape(&instrumented.display().to_string()),
        instrumented_sha256,
        json_escape(&store.display().to_string()),
        json_escape(&source.display().to_string()),
        source_digest,
        blake3::hash(schedule.as_bytes()).to_hex(),
        reset_wall_ns,
        forecast_wall_ns,
    );
    durable_write(receipt, json.as_bytes())?;
    println!(
        "stage1m-parity-readiness status=PASS receipt={} reset_wall_ns={} forecast_wall_ns={}",
        receipt.display(),
        reset_wall_ns,
        forecast_wall_ns
    );
    Ok(())
}

pub(in crate::stage1_materialize) fn parity_schedule_json() -> String {
    "{\"schema\":\"layerfs-stage1m-parity-schedule-v1\",\"size_mib\":24,\"pairs\":[[\"H\",\"I\"],[\"I\",\"H\"],[\"I\",\"H\"],[\"H\",\"I\"]],\"warmups\":8,\"measured\":8}\n".to_owned()
}
