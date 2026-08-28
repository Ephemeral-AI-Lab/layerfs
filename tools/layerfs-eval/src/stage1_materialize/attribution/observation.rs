use super::super::contract::{EvalResult, FIXTURE_MODE};
use super::super::error::{display_error, io_error};
use super::super::evidence::process::{current_rss_bytes, fd_count, process_usage};
use super::super::row::contract::EngineDelta;
use super::super::row::contract::Row;
use super::super::row::run::run_one;
use super::contract::AttributionArm;
use super::native::{verify_native_destination, TimedSink};
use crate::legacy_full::{LayerFs, NativeMetadata, NativeXattrs};
use std::fs::{self, File};
use std::os::unix::fs::MetadataExt;
use std::path::Path;
use std::time::Instant;

pub(in crate::stage1_materialize) struct AttributionObservation {
    pub(in crate::stage1_materialize) row: Row,
    pub(in crate::stage1_materialize) sink_write_calls: u64,
    pub(in crate::stage1_materialize) sink_write_ns: u64,
    pub(in crate::stage1_materialize) digest_sink_hash_bytes: Option<u64>,
}

#[allow(clippy::too_many_arguments)]
pub(in crate::stage1_materialize) fn run_attribution_one(
    fs: &LayerFs,
    root: crate::legacy_full::RootId,
    arm: AttributionArm,
    source: &Path,
    source_digest: &str,
    source_metadata: &fs::Metadata,
    expected_bytes: u64,
    destination: &Path,
    process_fd_baseline: u64,
) -> EvalResult<AttributionObservation> {
    if arm == AttributionArm::Complete {
        return Ok(AttributionObservation {
            row: run_one(
                fs,
                root,
                source,
                source_digest,
                source_metadata,
                expected_bytes,
                destination,
                process_fd_baseline,
            )?,
            sink_write_calls: 0,
            sink_write_ns: 0,
            digest_sink_hash_bytes: None,
        });
    }
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
    let mut sink = TimedSink::new(arm == AttributionArm::Digest);
    let mut native_source = (arm == AttributionArm::Native)
        .then(|| File::open(source).map_err(io_error))
        .transpose()?;
    let native_metadata = NativeMetadata {
        mode: FIXTURE_MODE,
        mtime_seconds: source_metadata.mtime(),
        mtime_nanoseconds: source_metadata.mtime_nsec() as u32,
        xattrs: NativeXattrs::new(),
        acl: None,
        bsd_flags: 0,
    };
    let product_started = Instant::now();
    let operation = match arm {
        AttributionArm::Null | AttributionArm::Digest => fs
            .materialize_authenticated_to(root, &mut sink)
            .map_err(display_error)?,
        AttributionArm::Native => fs
            .native_durable_output(
                destination,
                b"payload.bin",
                &native_metadata,
                expected_bytes,
                native_source
                    .take()
                    .ok_or_else(|| "native source is unavailable".to_owned())?,
            )
            .map_err(display_error)?,
        AttributionArm::Complete => unreachable!(),
    };
    let product_wall_ns = product_started.elapsed().as_nanos();
    let usage_after = process_usage()?;
    let fd_after = fd_count()?;
    let rss_current_bytes = current_rss_bytes()?;
    let after = fs.counter_snapshot().map_err(display_error)?;
    let engine = EngineDelta::between(&before, &after)?;

    let oracle_started = Instant::now();
    let output_digest = match arm {
        AttributionArm::Null => {
            if sink.bytes != expected_bytes {
                return Err("null sink byte equation failed".to_owned());
            }
            "NotApplicable".to_owned()
        }
        AttributionArm::Digest => {
            let digest = sink.digest()?;
            if sink.bytes != expected_bytes || digest != source_digest {
                return Err("digest sink oracle failed".to_owned());
            }
            digest
        }
        AttributionArm::Native => verify_native_destination(
            fs,
            destination,
            source_digest,
            &native_metadata,
            expected_bytes,
        )?,
        AttributionArm::Complete => unreachable!(),
    };
    let oracle_wall_ns = oracle_started.elapsed().as_nanos();
    let cleanup_started = Instant::now();
    if destination.exists() {
        fs::remove_dir_all(destination).map_err(io_error)?;
    }
    let cleanup_wall_ns = cleanup_started.elapsed().as_nanos();
    if destination.exists() {
        return Err("attribution cleanup left residue".to_owned());
    }
    let projection_total = fs
        .projection_facts()
        .checked_delta(projection_before)
        .ok_or_else(|| "projection facts moved backwards".to_owned())?;
    let scratch_connections_peak = operation.scratch_tables;
    let total_connections_peak = after
        .active_connections
        .checked_add(scratch_connections_peak)
        .ok_or_else(|| "peak connection count overflow".to_owned())?;
    let row_wall_ns = row_started.elapsed().as_nanos();
    Ok(AttributionObservation {
        row: Row {
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
            scratch_connections_current: 0,
            scratch_connections_peak,
            total_connections_current: after.active_connections,
            total_connections_peak,
            projection_total,
            fd_terminal: None,
            connections_terminal: None,
            scratch_connections_terminal: None,
            total_connections_terminal: None,
            process_fd_baseline,
        },
        sink_write_calls: sink.write_calls,
        sink_write_ns: sink.write_ns,
        digest_sink_hash_bytes: (arm == AttributionArm::Digest).then_some(sink.bytes),
    })
}
