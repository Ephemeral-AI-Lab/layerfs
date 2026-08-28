use super::artifact::display_error;
use super::counter_validation::{
    engine_delta, verify_direct_read, verify_exact_noop, verify_operation_resources,
    verify_read_only_engine, verify_state_change,
};
use super::environment::base;
use super::model::{Campaign, DigestSink, EditCase};
use super::operation_evidence::{clone_json, counters_json, engine_json};
use super::resource_evidence::process_resources;
use super::root_validation::splice_digest;
use crate::legacy_full::NativeRoute;
use crate::stage1_fixture::{edit_bytes, EvalResult, Master, FILE_BYTES, FILE_PATH};
use std::time::Instant;
pub(crate) fn run_a09(campaign: &mut Campaign<'_>, master: &Master) -> EvalResult<()> {
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
pub(crate) fn run_a10_a12(campaign: &mut Campaign<'_>, master: &Master) -> EvalResult<()> {
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
