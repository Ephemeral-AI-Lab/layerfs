use super::artifact::{display_error, json_u128_array};
use super::counter_validation::{engine_delta, verify_direct_read, verify_read_only_engine};
use super::environment::base;
use super::model::{Campaign, DigestSink, MIB};
use super::operation_evidence::{clone_json, counters_json, engine_json, json_u64_array};
use super::resource_evidence::timer_residual;
use super::root_validation::merge_counters;
use crate::legacy_full::OperationDiagnostics;
use crate::stage1_fixture::{
    expected_bytes, EvalResult, Master, FILE_BYTES, FILE_PATH, RANDOM_RANGE_BYTES,
};
use std::time::Instant;
pub(crate) fn run_a01(campaign: &mut Campaign<'_>, master: &Master) -> EvalResult<()> {
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
pub(crate) fn run_a02(campaign: &mut Campaign<'_>, master: &Master) -> EvalResult<()> {
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
