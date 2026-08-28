use super::artifact::display_error;
use super::campaign_write::edit_cases;
use super::counter_validation::{
    engine_delta, locality_evidence_json, verify_logical_locality, verify_native_edit_shape,
    verify_operation_resources, verify_state_change,
};
use super::environment::base;
use super::model::{Campaign, DigestSink, EditCase};
use super::operation_evidence::{clone_json, counters_json, edit_case_json, engine_json};
use super::resource_evidence::timer_residual;
use super::root_validation::{
    canonical_digest, edit_result_len, expected_ref, merge_counters, splice_digest,
    verify_old_root_range,
};
use crate::stage1_fixture::{EvalResult, Master, FILE_PATH};
use std::time::Instant;
pub(crate) fn run_edit_matrix(campaign: &mut Campaign<'_>, master: &Master) -> EvalResult<()> {
    for case in edit_cases() {
        for sample in 1..=3 {
            run_logical_edit(campaign, master, &case, sample)?;
            run_native_edit(campaign, master, &case, sample)?;
        }
    }
    Ok(())
}
pub(crate) fn run_logical_edit(
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
pub(crate) fn run_native_edit(
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
