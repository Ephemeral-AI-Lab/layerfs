use super::artifact::{display_error, io_error};
use super::counter_validation::{engine_delta, verify_operation_resources, verify_state_change};
use super::environment::base;
use super::model::{BoundedRead, Campaign, EditCase};
use super::operation_evidence::{clone_json, counters_json, engine_json, option_u64_json};
use super::root_validation::{canonical_digest, expected_ref};
use crate::stage1_fixture::{edit_bytes, input_path, EvalResult, Master, FILE_BYTES, FILE_PATH};
use std::fs::File;
use std::time::Instant;
pub(crate) fn run_stream_write(
    campaign: &mut Campaign<'_>,
    master: &Master,
    id: &str,
    base_name: &str,
    replacement: bool,
) -> EvalResult<()> {
    let expected = base(master, base_name)?;
    let wanted_digest = if replacement {
        &master.replacement_digest
    } else {
        &master.raw_digest
    };
    for sample in 1..=3 {
        let complete_started = Instant::now();
        let attempt = campaign.attempt(base_name, expected)?;
        let clone = attempt.clone.clone();
        let (opened, open_wall) = campaign.open(&attempt, expected)?;
        let before = opened.fs.counter_snapshot().map_err(display_error)?;
        let input = BoundedRead(File::open(input_path(replacement)).map_err(io_error)?);
        let operation_started = Instant::now();
        let (state, counters) = opened
            .fs
            .replace_file_observed(&expected_ref(expected), FILE_PATH, input)
            .map_err(display_error)?;
        let operation_wall = operation_started.elapsed().as_nanos();
        campaign.operation_wall(operation_wall)?;
        let after = opened.fs.diagnostics().map_err(display_error)?;
        campaign.store_database(after.database_bytes);
        let engine = engine_delta(&before, &after)?;
        verify_state_change(&engine, 1)?;
        verify_operation_resources(&counters)?;
        campaign.bind_output_root(id, state.root)?;
        let post_started = Instant::now();
        if opened.fs.current_head("main").map_err(display_error)? != state {
            return Err(format!("{id} did not publish its exact RefState"));
        }
        let (bytes, digest, _) = canonical_digest(&opened.fs, state.root)?;
        if bytes != FILE_BYTES || &digest != wanted_digest {
            return Err(format!("{id} sample {sample} output mismatch"));
        }
        let post_wall = post_started.elapsed().as_nanos();
        campaign.postcheck_wall(post_wall)?;
        campaign.metric(id, operation_wall, Some(FILE_BYTES))?;
        campaign.data.last_q_terminal_bytes = Some(after.operation_q_current_bytes);
        drop(opened);
        let cleanup_wall = campaign.cleanup(attempt)?;
        campaign.row(format!(
            "{{\"id\":\"{id}\",\"sample\":{sample},\"cache\":\"cold-destination\",\"timing\":{{\"reset_ns\":{},\"open_ns\":{open_wall},\"operation_wall_ns\":{operation_wall},\"attributed_wall_ns\":{operation_wall},\"unattributed_wall_ns\":0,\"postcheck_ns\":{post_wall},\"cleanup_ns\":{cleanup_wall},\"complete_sample_wall_ns\":{}}},\"oracle\":{{\"bytes\":{bytes},\"blake3\":\"{digest}\",\"accepted_ref\":\"{}\"}},\"store_database_bytes\":{},\"clone\":{},\"operation_counters\":{},\"engine_delta\":{}}}",
            clone.wall_ns,
            complete_started.elapsed().as_nanos(),
            state.root,
            option_u64_json(after.database_bytes),
            clone_json(&clone),
            counters_json(&counters),
            engine_json(&engine),
        ))?;
    }
    Ok(())
}
pub(crate) fn edit_cases() -> Vec<EditCase> {
    let insert_base = FILE_BYTES - 8_192;
    let append_base = FILE_BYTES - 4_096;
    let delete_start = ((FILE_BYTES * 2 / 3) / 4_096) * 4_096;
    vec![
        EditCase {
            id: "A04",
            base: "overwrite",
            base_len: FILE_BYTES,
            start: FILE_BYTES / 2 - 2_048,
            delete_len: 4_096,
            replacement: edit_bytes(0x44, 4_096),
        },
        EditCase {
            id: "A05",
            base: "insert",
            base_len: insert_base,
            start: insert_base / 2 - 4_096,
            delete_len: 0,
            replacement: edit_bytes(0x45, 8_192),
        },
        EditCase {
            id: "A06",
            base: "delete",
            base_len: FILE_BYTES,
            start: delete_start,
            delete_len: 4_096,
            replacement: Vec::new(),
        },
        EditCase {
            id: "A07",
            base: "append",
            base_len: append_base,
            start: append_base,
            delete_len: 0,
            replacement: edit_bytes(0x47, 4_096),
        },
        EditCase {
            id: "A08",
            base: "truncate",
            base_len: FILE_BYTES,
            start: FILE_BYTES - 4_096,
            delete_len: 4_096,
            replacement: Vec::new(),
        },
    ]
}
