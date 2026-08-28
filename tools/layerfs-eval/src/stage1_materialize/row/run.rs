use super::super::contract::{EvalResult, FILE_PATH, FIXTURE_MODE};
use super::super::error::{display_error, io_error};
use super::super::evidence::digest::digest_file;
use super::super::evidence::process::{current_rss_bytes, fd_count, process_usage};
use super::contract::{EngineDelta, Row};
use crate::legacy_full::{ExternalWorkspace, LayerFs, OperationDiagnostics};
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::Path;
use std::time::Instant;

#[allow(clippy::too_many_arguments)]
pub(in crate::stage1_materialize) fn run_one(
    fs: &LayerFs,
    root: crate::legacy_full::RootId,
    source: &Path,
    source_digest: &str,
    source_metadata: &fs::Metadata,
    expected_bytes: u64,
    destination: &Path,
    process_fd_baseline: u64,
) -> EvalResult<Row> {
    if destination.exists() {
        return Err(format!(
            "fresh destination already exists: {}",
            destination.display()
        ));
    }
    let row_started = Instant::now();
    let before = fs.counter_snapshot().map_err(display_error)?;
    let projection_before = fs.projection_facts();
    let usage_before = process_usage()?;
    let fd_before = fd_count()?;
    let product_started = Instant::now();
    let (mut external, mut operation) = fs
        .materialize_external_observed(root, destination)
        .map_err(display_error)?;
    let product_wall_ns = product_started.elapsed().as_nanos();
    let usage_after = process_usage()?;
    let fd_after = fd_count()?;
    let rss_current_bytes = current_rss_bytes()?;
    let after = fs.counter_snapshot().map_err(display_error)?;
    let engine = EngineDelta::between(&before, &after)?;
    let scratch_connections_current = external.scratch_connection_count();
    let scratch_connections_peak = operation.scratch_tables;
    let total_connections_current = after
        .active_connections
        .checked_add(scratch_connections_current)
        .ok_or_else(|| "current connection count overflow".to_owned())?;
    let total_connections_peak = after
        .active_connections
        .checked_add(scratch_connections_peak)
        .ok_or_else(|| "peak connection count overflow".to_owned())?;

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
    operation = merge_terminal_cleanup(
        operation,
        external.discard_observed().map_err(display_error)?,
    )?;
    drop(external);
    let projection_total = fs
        .projection_facts()
        .checked_delta(projection_before)
        .ok_or_else(|| "projection facts moved backwards".to_owned())?;
    fs::remove_dir_all(destination).map_err(io_error)?;
    let cleanup_wall_ns = cleanup_started.elapsed().as_nanos();
    if destination.exists() {
        return Err("destination cleanup left residue".to_owned());
    }
    let row_wall_ns = row_started.elapsed().as_nanos();
    Ok(Row {
        product_wall_ns,
        row_wall_ns,
        oracle_wall_ns,
        cleanup_wall_ns,
        output_digest,
        engine,
        operation,
        user_cpu_ns: usage_after
            .user_ns
            .checked_sub(usage_before.user_ns)
            .ok_or_else(|| "user CPU moved backwards".to_owned())?,
        system_cpu_ns: usage_after
            .system_ns
            .checked_sub(usage_before.system_ns)
            .ok_or_else(|| "system CPU moved backwards".to_owned())?,
        rss_peak_bytes: usage_after.maximum_rss_bytes,
        rss_current_bytes,
        fd_before,
        fd_after,
        active_connections: after.active_connections,
        scratch_connections_current,
        scratch_connections_peak,
        total_connections_current,
        total_connections_peak,
        projection_total,
        fd_terminal: None,
        connections_terminal: None,
        scratch_connections_terminal: None,
        total_connections_terminal: None,
        process_fd_baseline,
    })
}

pub(in crate::stage1_materialize) fn merge_terminal_cleanup(
    operation: OperationDiagnostics,
    cleanup: OperationDiagnostics,
) -> EvalResult<OperationDiagnostics> {
    operation.merge(cleanup).map_err(display_error)
}

pub(in crate::stage1_materialize) fn verify_destination(
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
