use super::artifact::{display_error, json_u128_array};
use super::counter_validation::{engine_delta, verify_state_change};
use super::environment::base;
use super::model::{Campaign, MIB};
use super::operation_evidence::{clone_json, counters_json, engine_json, option_growth_json};
use super::resource_evidence::timer_residual;
use super::root_validation::{expected_ref, history_edit, history_expected_range, merge_counters};
use crate::legacy_full::{IntegrityMode, OperationDiagnostics};
use crate::stage1_fixture::{EvalResult, Master, FILE_PATH, RANDOM_RANGE_BYTES};
use std::time::Instant;
pub(crate) fn run_a13(campaign: &mut Campaign<'_>, master: &Master) -> EvalResult<()> {
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
pub(crate) fn run_a14(campaign: &mut Campaign<'_>, master: &Master) -> EvalResult<()> {
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
