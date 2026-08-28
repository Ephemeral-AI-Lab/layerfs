use super::super::contract::EvalResult;
use super::super::error::{display_error, io_error};
use super::super::evidence::digest::digest_file;
use super::contract::AttributionArm;
use super::equations::trust_equation;
use super::observation::AttributionObservation;
use super::projection::{
    attribution_timer_equation, engine_sql, scratch_sql, successful_projection_facts_exact,
};
use crate::legacy_full::{IntegrityMode, LayerFs, NativeMetadata};
use std::fs;
use std::io::Write;
use std::path::Path;
use std::time::Instant;

pub(in crate::stage1_materialize) fn verify_native_destination(
    fs: &LayerFs,
    destination: &Path,
    source_digest: &str,
    expected_metadata: &NativeMetadata,
    expected_bytes: u64,
) -> EvalResult<String> {
    let output = destination.join("payload.bin");
    let metadata = fs::metadata(&output).map_err(io_error)?;
    if !metadata.is_file() || metadata.len() != expected_bytes {
        return Err("native durable output length mismatch".to_owned());
    }
    let digest = digest_file(&output)?;
    if digest != source_digest {
        return Err("native durable output digest mismatch".to_owned());
    }
    let external = fs.open_external(destination).map_err(display_error)?;
    let actual = external
        .read_metadata("payload.bin")
        .map_err(display_error)?;
    if &actual != expected_metadata {
        return Err("native durable output metadata mismatch".to_owned());
    }
    drop(external);
    Ok(digest)
}

pub(in crate::stage1_materialize) struct TimedSink {
    pub(in crate::stage1_materialize) hasher: Option<blake3::Hasher>,
    pub(in crate::stage1_materialize) bytes: u64,
    pub(in crate::stage1_materialize) write_calls: u64,
    pub(in crate::stage1_materialize) write_ns: u64,
}

impl TimedSink {
    pub(in crate::stage1_materialize) fn new(digest: bool) -> Self {
        Self {
            hasher: digest.then(blake3::Hasher::new),
            bytes: 0,
            write_calls: 0,
            write_ns: 0,
        }
    }

    pub(in crate::stage1_materialize) fn digest(&self) -> EvalResult<String> {
        self.hasher
            .as_ref()
            .map(|hasher| hasher.clone().finalize().to_hex().to_string())
            .ok_or_else(|| "digest requested from null sink".to_owned())
    }
}

impl Write for TimedSink {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        let started = Instant::now();
        if let Some(hasher) = self.hasher.as_mut() {
            hasher.update(bytes);
        } else {
            std::hint::black_box(bytes);
        }
        self.write_ns = self
            .write_ns
            .checked_add(started.elapsed().as_nanos() as u64)
            .ok_or_else(|| std::io::Error::other("sink timer overflow"))?;
        self.bytes = self
            .bytes
            .checked_add(bytes.len() as u64)
            .ok_or_else(|| std::io::Error::other("sink byte overflow"))?;
        self.write_calls = self
            .write_calls
            .checked_add(1)
            .ok_or_else(|| std::io::Error::other("sink call overflow"))?;
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

pub(in crate::stage1_materialize) fn validate_attribution_observation(
    arm: AttributionArm,
    expected_bytes: u64,
    observation: &AttributionObservation,
    mode: IntegrityMode,
) -> EvalResult<()> {
    let row = &observation.row;
    let operation = &row.operation;
    let engine_sql = engine_sql(&row.engine)?;
    let scratch_sql = scratch_sql(operation)?;
    let content_bytes = operation
        .content_payload_bytes_read()
        .ok_or_else(|| "content payload accounting underflow".to_owned())?;
    let trust_exact = trust_equation(mode, &row.engine);
    let common = engine_sql == row.engine.statements
        && scratch_sql == operation.scratch_statements
        && successful_projection_facts_exact(operation.projection)
        && successful_projection_facts_exact(row.projection_total)
        && trust_exact
        && row.engine.busy_events == 0
        && row.engine.locked_events == 0
        && row.engine.publication_commits == 0
        && operation.rope.cdc_bytes_scanned == 0
        && operation.rematerializations == 0
        && operation.full_fallback_files == 0
        && operation.operation_q_high_water_bytes < 8 * 1024 * 1024
        && operation.operation_q_terminal_bytes == 0
        && operation.owned_temp_terminal == 0
        && operation.descriptor_spool_bytes_terminal == 0
        && row.active_connections == 1
        && row.scratch_connections_peak <= 1
        && row.total_connections_peak <= 2
        && row.total_connections_current
            == row
                .active_connections
                .checked_add(row.scratch_connections_current)
                .ok_or_else(|| "current connection equation overflow".to_owned())?
        && row.fd_before <= 24
        && row.fd_after <= 24
        && row.rss_peak_bytes <= 32 * 1024 * 1024
        && row.rss_current_bytes <= 32 * 1024 * 1024
        && row.engine.payload_batch_maximum <= 64;
    let arm_exact = match arm {
        AttributionArm::Complete => {
            content_bytes == expected_bytes
                && operation.native.bytes_written == expected_bytes
                && operation.projection.content_write.bytes == expected_bytes
                && observation.digest_sink_hash_bytes.is_none()
        }
        AttributionArm::Null => {
            content_bytes == expected_bytes
                && operation.native == Default::default()
                && operation.projection.content_write.attempts == 0
                && observation.digest_sink_hash_bytes.is_none()
                && (expected_bytes == 0 || observation.sink_write_calls > 0)
        }
        AttributionArm::Digest => {
            content_bytes == expected_bytes
                && operation.native == Default::default()
                && operation.projection.content_write.attempts == 0
                && observation.digest_sink_hash_bytes == Some(expected_bytes)
                && (expected_bytes == 0 || observation.sink_write_calls > 0)
        }
        AttributionArm::Native => {
            content_bytes == 0
                && row.engine.fetched_rows == 0
                && operation.scratch_statements == 0
                && operation.native.bytes_written == expected_bytes
                && operation.projection.content_write.bytes == expected_bytes
                && observation.digest_sink_hash_bytes.is_none()
        }
    };
    if !common || !arm_exact {
        return Err(format!(
            "{} attribution equation failed (common={common}, arm={arm_exact})",
            arm.name()
        ));
    }
    let (leaf_ns, _, residual_ns) = attribution_timer_equation(arm, observation)?;
    let tolerance = 500_000_u128.max(row.product_wall_ns / 100);
    if residual_ns.unsigned_abs() > tolerance || leaf_ns == 0 && row.product_wall_ns != 0 {
        return Err(format!(
            "{} attribution timer equation failed: residual={residual_ns}",
            arm.name()
        ));
    }
    Ok(())
}
