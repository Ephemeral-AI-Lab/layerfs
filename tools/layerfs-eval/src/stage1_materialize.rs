use layerfs_sdk::{Diagnostics, ExternalWorkspace, LayerFs, OperationDiagnostics};
use std::ffi::OsStr;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::os::unix::fs::MetadataExt;
use std::path::Path;
use std::process::Command;
use std::time::Instant;

const FILE_PATH: &str = "data/payload.bin";
const BUFFER_BYTES: usize = 1024 * 1024;
const FIXTURE_MODE: u32 = 0o644;

type EvalResult<T> = Result<T, String>;

pub fn hash(path: &Path) -> EvalResult<()> {
    let output = Command::new("/usr/bin/shasum")
        .args(["-a", "256"])
        .arg(path)
        .output()
        .map_err(io_error)?;
    if !output.status.success() {
        return Err("shasum failed".to_owned());
    }
    let sha256 = String::from_utf8(output.stdout)
        .map_err(display_error)?
        .split_whitespace()
        .next()
        .ok_or_else(|| "shasum returned no digest".to_owned())?
        .to_owned();
    println!(
        "{{\"path\":\"{}\",\"bytes\":{},\"sha256\":\"{}\",\"blake3\":\"{}\"}}",
        path.display(),
        fs::metadata(path).map_err(io_error)?.len(),
        sha256,
        digest_file(path)?,
    );
    Ok(())
}

pub fn parity_row(
    store: &Path,
    source: &Path,
    size_mib: &OsStr,
    work: &Path,
    identity: &OsStr,
) -> EvalResult<()> {
    let size_mib = size_mib
        .to_str()
        .ok_or_else(|| "size-mib is not UTF-8".to_owned())?
        .parse::<u64>()
        .map_err(|error| format!("invalid size-mib: {error}"))?;
    let identity = identity
        .to_str()
        .filter(|value| {
            !value.is_empty()
                && value
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
        })
        .ok_or_else(|| "identity must be a nonempty ASCII identifier".to_owned())?;
    let expected_bytes = size_mib
        .checked_mul(1024 * 1024)
        .ok_or_else(|| "fixture byte length overflow".to_owned())?;
    if !matches!(size_mib, 0 | 24 | 96) {
        return Err("size-mib must be exactly 0, 24, or 96".to_owned());
    }
    let source_metadata = fs::metadata(source).map_err(io_error)?;
    if !source_metadata.is_file() || source_metadata.len() != expected_bytes {
        return Err(format!(
            "source fixture length mismatch: expected {expected_bytes}, got {}",
            source_metadata.len()
        ));
    }
    let source_digest = digest_file(source)?;
    fs::create_dir(work).map_err(io_error)?;
    let store_clone = work.join("store");
    clone_store(store, &store_clone)?;

    let opened = LayerFs::open(&store_clone).map_err(display_error)?;
    let root = opened.ref_state.root;
    let primer = run_one(
        &opened.fs,
        root,
        source,
        &source_digest,
        &source_metadata,
        expected_bytes,
        &work.join("primer"),
    )?;
    print_row("warmup", identity, size_mib, root, &source_digest, &primer)?;
    std::io::stdout().flush().map_err(io_error)?;

    let measured = run_one(
        &opened.fs,
        root,
        source,
        &source_digest,
        &source_metadata,
        expected_bytes,
        &work.join("measured"),
    )?;
    print_row(
        "measured",
        identity,
        size_mib,
        root,
        &source_digest,
        &measured,
    )?;
    std::io::stdout().flush().map_err(io_error)?;
    drop(opened);
    fs::remove_dir_all(&store_clone).map_err(io_error)?;
    fs::remove_dir(work).map_err(io_error)?;
    Ok(())
}

fn clone_store(source: &Path, destination: &Path) -> EvalResult<()> {
    let status = Command::new("/bin/cp")
        .arg("-cR")
        .arg(source)
        .arg(destination)
        .status()
        .map_err(io_error)?;
    if !status.success() {
        return Err(format!("APFS Store clone exited {status}"));
    }
    crate::stage1_fixture::make_writable(destination)
}

struct Row {
    product_wall_ns: u128,
    oracle_wall_ns: u128,
    cleanup_wall_ns: u128,
    output_digest: String,
    engine: EngineDelta,
    operation: OperationDiagnostics,
}

fn run_one(
    fs: &LayerFs,
    root: layerfs_sdk::RootId,
    source: &Path,
    source_digest: &str,
    source_metadata: &fs::Metadata,
    expected_bytes: u64,
    destination: &Path,
) -> EvalResult<Row> {
    if destination.exists() {
        return Err(format!(
            "fresh destination already exists: {}",
            destination.display()
        ));
    }
    let before = fs.counter_snapshot().map_err(display_error)?;
    let product_started = Instant::now();
    let (external, operation) = fs
        .materialize_external_observed(root, destination)
        .map_err(display_error)?;
    let product_wall_ns = product_started.elapsed().as_nanos();
    let after = fs.counter_snapshot().map_err(display_error)?;
    let engine = EngineDelta::between(&before, &after)?;

    let oracle_started = Instant::now();
    let output_digest = verify_destination(
        &external,
        source,
        source_digest,
        source_metadata,
        expected_bytes,
    )?;
    let oracle_wall_ns = oracle_started.elapsed().as_nanos();

    let cleanup_started = Instant::now();
    drop(external);
    fs::remove_dir_all(destination).map_err(io_error)?;
    let cleanup_wall_ns = cleanup_started.elapsed().as_nanos();
    if destination.exists() {
        return Err("destination cleanup left residue".to_owned());
    }
    Ok(Row {
        product_wall_ns,
        oracle_wall_ns,
        cleanup_wall_ns,
        output_digest,
        engine,
        operation,
    })
}

fn verify_destination(
    external: &ExternalWorkspace,
    source: &Path,
    source_digest: &str,
    source_metadata: &fs::Metadata,
    expected_bytes: u64,
) -> EvalResult<String> {
    let output = external.path().join(FILE_PATH);
    let output_metadata = fs::metadata(&output).map_err(io_error)?;
    if !output_metadata.is_file() || output_metadata.len() != expected_bytes {
        return Err("materialized output length mismatch".to_owned());
    }
    let output_digest = digest_file(&output)?;
    if output_digest != source_digest || digest_file(source)? != source_digest {
        return Err("materialized output digest mismatch".to_owned());
    }
    let native = external.read_metadata(FILE_PATH).map_err(display_error)?;
    if native.mode != FIXTURE_MODE
        || native.mtime_seconds != source_metadata.mtime()
        || native.mtime_nanoseconds != source_metadata.mtime_nsec() as u32
        || !native.xattrs.is_empty()
        || native.acl.is_some()
        || native.bsd_flags != 0
    {
        return Err("materialized output metadata mismatch".to_owned());
    }
    let data = external.path().join("data");
    if fs::read_dir(external.path())
        .map_err(io_error)?
        .filter_map(Result::ok)
        .count()
        != 1
        || fs::read_dir(data)
            .map_err(io_error)?
            .filter_map(Result::ok)
            .count()
            != 1
    {
        return Err("materialized destination contains extra user entries".to_owned());
    }
    Ok(output_digest)
}

fn digest_file(path: &Path) -> EvalResult<String> {
    let mut file = File::open(path).map_err(io_error)?;
    let mut hasher = blake3::Hasher::new();
    let mut buffer = vec![0_u8; BUFFER_BYTES];
    loop {
        let count = file.read(&mut buffer).map_err(io_error)?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    Ok(hasher.finalize().to_hex().to_string())
}

#[derive(Clone, Copy)]
struct EngineDelta {
    statements: u64,
    integrity_statements: u64,
    busy_events: u64,
    locked_events: u64,
    fetched_rows: u64,
    authentication_passes: u64,
    role_decode_passes: u64,
    object_bytes_read: u64,
    payload_batch_queries: u64,
    payload_batch_references: u64,
    payload_batch_maximum: u64,
    publication_commits: u64,
}

impl EngineDelta {
    fn between(before: &Diagnostics, after: &Diagnostics) -> EvalResult<Self> {
        Ok(Self {
            statements: delta(after.statements, before.statements, "statements")?,
            integrity_statements: delta(
                after.integrity_statements,
                before.integrity_statements,
                "integrity_statements",
            )?,
            busy_events: delta(after.busy_events, before.busy_events, "busy_events")?,
            locked_events: delta(after.locked_events, before.locked_events, "locked_events")?,
            fetched_rows: delta(after.fetched_rows, before.fetched_rows, "fetched_rows")?,
            authentication_passes: delta(
                after.fetched_row_authentication_passes,
                before.fetched_row_authentication_passes,
                "authentication_passes",
            )?,
            role_decode_passes: delta(
                after.fetched_row_role_decode_passes,
                before.fetched_row_role_decode_passes,
                "role_decode_passes",
            )?,
            object_bytes_read: delta(
                after.object_bytes_read,
                before.object_bytes_read,
                "object_bytes_read",
            )?,
            payload_batch_queries: delta(
                after.payload_batch_queries,
                before.payload_batch_queries,
                "payload_batch_queries",
            )?,
            payload_batch_references: delta(
                after.payload_batch_references,
                before.payload_batch_references,
                "payload_batch_references",
            )?,
            payload_batch_maximum: after.payload_batch_maximum,
            publication_commits: delta(
                after.publication_commits,
                before.publication_commits,
                "publication_commits",
            )?,
        })
    }
}

fn print_row(
    kind: &str,
    identity: &str,
    size_mib: u64,
    root: layerfs_sdk::RootId,
    source_digest: &str,
    row: &Row,
) -> EvalResult<()> {
    let native = row.operation.native;
    let (operation_label, source_conditioning) = if kind == "warmup" {
        ("first_open_fresh_destination", "fresh_open_after_scrub")
    } else {
        (
            "same_open_warmed_source_fresh_destination",
            "same_open_after_primer",
        )
    };
    println!(
        concat!(
            "{{\"schema\":\"layerfs-stage1m-parity-row-v1\",",
            "\"status\":\"PASS\",\"row_kind\":\"{}\",\"identity\":\"{}\",",
            "\"operation_label\":\"{}\",\"source_conditioning\":\"{}\",",
            "\"controlled_device_cold\":false,\"incremental_refresh\":false,",
            "\"size_mib\":{},\"logical_bytes\":{},\"root\":\"{}\",",
            "\"source_digest\":\"{}\",\"output_digest\":\"{}\",",
            "\"product_operation_wall_ns\":{},\"oracle_wall_ns\":{},",
            "\"cleanup_wall_ns\":{},",
            "\"engine\":{{\"statements\":{},\"integrity_statements\":{},",
            "\"busy_events\":{},\"locked_events\":{},\"fetched_rows\":{},",
            "\"authentication_passes\":{},\"role_decode_passes\":{},",
            "\"object_bytes_read\":{},\"payload_batch_queries\":{},",
            "\"payload_batch_references\":{},\"payload_batch_maximum\":{},",
            "\"publication_commits\":{}}},",
            "\"scratch\":{{\"tables\":{},\"statements\":{},\"rows\":{},",
            "\"high_water_bytes\":{}}},",
            "\"native\":{{\"bytes_written\":{},\"temp_calls\":{},",
            "\"sync_calls\":{},\"replace_calls\":{},\"metadata_calls\":{}}},",
            "\"operation_q_terminal_bytes\":{},\"residue\":0}}"
        ),
        kind,
        identity,
        operation_label,
        source_conditioning,
        size_mib,
        size_mib * 1024 * 1024,
        root,
        source_digest,
        row.output_digest,
        row.product_wall_ns,
        row.oracle_wall_ns,
        row.cleanup_wall_ns,
        row.engine.statements,
        row.engine.integrity_statements,
        row.engine.busy_events,
        row.engine.locked_events,
        row.engine.fetched_rows,
        row.engine.authentication_passes,
        row.engine.role_decode_passes,
        row.engine.object_bytes_read,
        row.engine.payload_batch_queries,
        row.engine.payload_batch_references,
        row.engine.payload_batch_maximum,
        row.engine.publication_commits,
        row.operation.scratch_tables,
        row.operation.scratch_statements,
        row.operation.scratch_rows,
        row.operation.scratch_high_water_bytes,
        native.bytes_written,
        native.temp_calls,
        native.sync_calls,
        native.replace_calls,
        native.metadata_calls,
        row.operation.operation_q_terminal_bytes,
    );
    Ok(())
}

fn delta(after: u64, before: u64, name: &str) -> EvalResult<u64> {
    after
        .checked_sub(before)
        .ok_or_else(|| format!("counter {name} moved backwards"))
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

    #[test]
    fn delta_rejects_backwards_counters() {
        assert_eq!(delta(9, 4, "x").unwrap(), 5);
        assert!(delta(4, 9, "x").is_err());
    }
}
