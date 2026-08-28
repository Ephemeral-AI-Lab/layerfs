use super::artifact::{display_error, json_u128_array};
use super::counter_validation::{
    engine_delta, locality_evidence_json, verify_logical_locality, verify_operation_resources,
    verify_state_change,
};
use super::environment::base;
use super::model::{Campaign, DigestSink, EditCase};
use super::operation_evidence::{clone_json, counters_json, edit_case_json, engine_json};
use super::resource_evidence::timer_residual;
use super::root_validation::{
    canonical_digest, expected_ref, merge_counters, splice_digest, verify_old_root_range,
};
use crate::legacy_full::OperationDiagnostics;
use crate::stage1_fixture::{
    edit_bytes, expected_bytes, EvalResult, Master, FILE_BYTES, FILE_PATH,
};
use std::time::Instant;
pub(crate) fn run_a15(campaign: &mut Campaign<'_>, master: &Master) -> EvalResult<()> {
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
pub(crate) fn run_a17(campaign: &mut Campaign<'_>, master: &Master) -> EvalResult<()> {
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
