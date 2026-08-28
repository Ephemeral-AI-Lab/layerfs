use super::artifact::{display_error, io_error, json_optional_u128, json_string, json_u128};
use super::limits::INITIAL_BYTES;
use super::schedule_model::{FrozenSchedule, ScheduledRow};
use super::summary_json_parse::{
    json_object_member_names, json_top_level_string, json_top_level_u128,
};
use crate::stage1_fixture::EvalResult;
use std::fs;
use std::path::Path;
#[derive(Clone, Debug)]
pub(crate) struct ParsedRow {
    pub(crate) json: String,
    pub(crate) row_id: String,
    pub(crate) row_group: String,
    pub(crate) operation: String,
    pub(crate) size_band: String,
    pub(crate) native_route: String,
    pub(crate) status: String,
    pub(crate) before_bytes: u64,
    pub(crate) after_bytes: u64,
    pub(crate) row_wall_ns: u128,
    pub(crate) row_residual_ns: u128,
}
pub(crate) fn revision_length(schedule: &FrozenSchedule, revision: u8) -> EvalResult<u64> {
    match revision {
        0 => Ok(INITIAL_BYTES),
        1..=30 => schedule
            .edits
            .get(usize::from(revision - 1))
            .map(|edit| edit.after_bytes)
            .ok_or_else(|| format!("missing frozen revision R{revision}")),
        31..=34 => schedule
            .bursts
            .get(usize::from(revision - 31))
            .and_then(|burst| burst.edits.last())
            .map(|edit| edit.after_bytes)
            .ok_or_else(|| format!("missing frozen burst revision R{revision}")),
        _ => Err(format!("invalid frozen revision R{revision}")),
    }
}
pub(crate) fn scheduled_lengths(
    schedule: &FrozenSchedule,
    row: &ScheduledRow,
) -> EvalResult<(u64, u64)> {
    if let Some(index) = row.edit_index {
        let edit = schedule
            .edits
            .get(index)
            .ok_or_else(|| format!("{} missing edit {index}", row.row_id))?;
        Ok((edit.before_bytes, edit.after_bytes))
    } else if let Some(index) = row.burst_index {
        let burst = schedule
            .bursts
            .get(index)
            .ok_or_else(|| format!("{} missing burst {index}", row.row_id))?;
        Ok((
            burst
                .edits
                .first()
                .ok_or_else(|| format!("{} empty burst", row.row_id))?
                .before_bytes,
            burst
                .edits
                .last()
                .ok_or_else(|| format!("{} empty burst", row.row_id))?
                .after_bytes,
        ))
    } else if let Some(session) = row.history_session {
        let length = revision_length(schedule, session * 5)?;
        Ok((length, length))
    } else if let Some(root) = row.milestone_root {
        let length = revision_length(schedule, root)?;
        Ok((length, length))
    } else {
        Ok((INITIAL_BYTES, INITIAL_BYTES))
    }
}
pub(crate) fn parse_rows(path: &Path, schedule: &FrozenSchedule) -> EvalResult<Vec<ParsedRow>> {
    let contents = fs::read_to_string(path).map_err(io_error)?;
    if !contents.ends_with('\n') {
        return Err("rows.jsonl is not newline terminated".to_owned());
    }
    let mut rows = Vec::new();
    for (index, line) in contents.lines().enumerate() {
        let expected = schedule
            .rows
            .get(index)
            .ok_or_else(|| "rows.jsonl has too many rows".to_owned())?;
        let status = json_top_level_string(line, "status")?;
        let (expected_before, expected_after) = scheduled_lengths(schedule, expected)?;
        if json_top_level_string(line, "schema")? != "layerfs-stage1.1-row-v1"
            || json_top_level_u128(line, "row_index")? != index as u128
            || json_top_level_string(line, "row_id")? != expected.row_id
            || json_top_level_string(line, "row_group")? != expected.row_group
            || json_top_level_u128(line, "sequence")? != u128::from(expected.sequence)
            || json_top_level_u128(line, "epoch")? != u128::from(expected.epoch)
            || json_top_level_string(line, "direction")? != expected.direction
            || json_top_level_string(line, "operation")? != expected.operation
            || json_top_level_string(line, "size_band")? != expected.size_band
            || json_top_level_u128(line, "before_bytes")? != u128::from(expected_before)
            || json_top_level_u128(line, "after_bytes")? != u128::from(expected_after)
            || !matches!(status.as_str(), "PASS" | "REVISE" | "FAIL")
        {
            return Err(format!("invalid retained row at index {index}"));
        }
        let top_level = json_object_member_names(line)?;
        for key in [
            "schema",
            "row_index",
            "row_id",
            "row_group",
            "sequence",
            "epoch",
            "direction",
            "operation",
            "size_band",
            "status",
            "before_bytes",
            "after_bytes",
            "edit",
            "sub_edits",
            "history_probes",
            "pre_ref",
            "post_ref",
            "native_route",
            "tree_level_before",
            "phases",
            "phase_counters",
            "row_wall_ns",
            "row_residual_ns",
            "counters",
            "native",
            "storage",
            "resources",
            "oracle",
            "unavailable",
            "error",
        ] {
            if !top_level.iter().any(|actual| actual == key) {
                return Err(format!(
                    "row {} missing common field {key}",
                    expected.row_id
                ));
            }
        }
        rows.push(ParsedRow {
            json: line.to_owned(),
            row_id: expected.row_id.clone(),
            row_group: json_top_level_string(line, "row_group")?,
            operation: json_top_level_string(line, "operation")?,
            size_band: json_top_level_string(line, "size_band")?,
            native_route: json_top_level_string(line, "native_route")?,
            status,
            before_bytes: u64::try_from(json_top_level_u128(line, "before_bytes")?)
                .map_err(display_error)?,
            after_bytes: u64::try_from(json_top_level_u128(line, "after_bytes")?)
                .map_err(display_error)?,
            row_wall_ns: json_top_level_u128(line, "row_wall_ns")?,
            row_residual_ns: json_top_level_u128(line, "row_residual_ns")?,
        });
    }
    if rows.len() != 47 {
        return Err(format!("rows.jsonl contains {} rows, not 47", rows.len()));
    }
    Ok(rows)
}
pub(crate) fn phase_wall(json: &str, name: &str) -> EvalResult<u128> {
    let needle = format!("\"name\":\"{name}\",\"wall_ns\":");
    let start = json
        .find(&needle)
        .map(|offset| offset + needle.len())
        .ok_or_else(|| format!("missing phase {name}"))?;
    parse_digits(&json[start..], &format!("phase {name}"))
}
pub(crate) fn json_all_u128(json: &str, key: &str) -> EvalResult<Vec<u128>> {
    let needle = format!("\"{key}\":");
    let mut values = Vec::new();
    let mut remaining = json;
    while let Some(offset) = remaining.find(&needle) {
        let start = offset + needle.len();
        values.push(parse_digits(&remaining[start..], key)?);
        remaining = &remaining[start..];
    }
    Ok(values)
}
pub(crate) fn parse_digits(value: &str, label: &str) -> EvalResult<u128> {
    let digits = value
        .chars()
        .take_while(char::is_ascii_digit)
        .collect::<String>();
    if digits.is_empty() {
        return Err(format!("invalid integer for {label}"));
    }
    digits.parse().map_err(display_error)
}
pub(crate) fn json_object<'a>(json: &'a str, key: &str) -> EvalResult<&'a str> {
    let needle = format!("\"{key}\":{{");
    let start = json
        .find(&needle)
        .map(|offset| offset + needle.len() - 1)
        .ok_or_else(|| format!("missing JSON object {key}"))?;
    let bytes = json.as_bytes();
    let mut depth = 0_u32;
    let mut string = false;
    let mut escaped = false;
    for (relative, byte) in bytes[start..].iter().copied().enumerate() {
        if string {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                string = false;
            }
            continue;
        }
        match byte {
            b'"' => string = true,
            b'{' => depth += 1,
            b'}' => {
                depth = depth
                    .checked_sub(1)
                    .ok_or_else(|| format!("JSON object {key} depth underflow"))?;
                if depth == 0 {
                    return Ok(&json[start..start + relative + 1]);
                }
            }
            _ => {}
        }
    }
    Err(format!("unterminated JSON object {key}"))
}
pub(crate) fn row_optional_u128(row: &ParsedRow, key: &str) -> EvalResult<Option<u128>> {
    let object = if matches!(
        key,
        "transactions_started"
            | "transactions_committed"
            | "transactions_rolled_back"
            | "statements"
            | "admission_transactions_started"
            | "admission_transactions_committed"
            | "admission_transactions_rolled_back"
            | "admission_statements"
            | "integrity_transactions_started"
            | "integrity_transactions_committed"
            | "integrity_transactions_rolled_back"
            | "integrity_statements"
            | "busy_events"
            | "locked_events"
            | "objects_validated"
            | "objects_created"
            | "objects_reused"
            | "object_bytes_read"
            | "object_bytes_written"
            | "fetched_rows"
            | "fetched_row_authentication_passes"
            | "fetched_row_role_decode_passes"
            | "new_object_authentication_passes"
            | "incumbent_authentication_passes"
            | "payload_batch_queries"
            | "payload_batch_references"
            | "payload_batch_maximum"
            | "put_lookup_statements"
            | "put_insert_statements"
            | "created_rows"
            | "reused_rows"
            | "publication_transactions_started"
            | "publication_transactions_rolled_back"
            | "publication_commits"
            | "publication_closure_passes"
            | "namespace_graph_verification_passes"
            | "scratch_tables"
            | "scratch_statements"
            | "scratch_rows"
            | "scratch_high_water_bytes"
            | "retained_roots_validated"
            | "cdc_bytes_scanned"
            | "payload_bytes_written"
            | "unaffected_payload_reads"
            | "unaffected_payload_writes"
            | "rope_nodes_read"
            | "rope_nodes_emitted"
            | "content_directory_nodes_emitted"
            | "workspace_materializations"
            | "workspace_reuses"
            | "rematerializations"
            | "descriptor_resets"
    ) {
        "counters"
    } else if matches!(
        key,
        "bytes_read"
            | "bytes_written"
            | "patch_bytes"
            | "suffix_bytes_shifted"
            | "clone_attempts"
            | "clone_successes"
            | "clone_fallbacks"
            | "full_fallback_files"
            | "files_created"
            | "files_replaced"
            | "files_removed"
            | "sync_regular_calls"
            | "sync_directory_calls"
    ) {
        "native"
    } else if matches!(
        key,
        "database_bytes"
            | "logical_engine_bytes"
            | "rollback_journal_bytes"
            | "temporary_file_bytes"
            | "database_growth_bytes"
            | "canonical_object_bytes_written"
            | "physical_to_canonical_amplification"
    ) {
        "storage"
    } else if matches!(
        key,
        "rss_current_bytes"
            | "rss_peak_bytes"
            | "operation_q_current_bytes"
            | "operation_q_high_water_bytes"
            | "operation_q_terminal_bytes"
            | "fd_current"
            | "active_store_connections"
            | "child_processes"
            | "owned_temp_entries"
            | "residue_entries"
            | "largest_buffer_bytes"
            | "page_size"
            | "cache_pages"
            | "cache_spill_pages"
            | "network_operations"
    ) {
        "resources"
    } else {
        return json_optional_u128(&row.json, key);
    };
    json_optional_u128(json_object(&row.json, object)?, key)
}
pub(crate) fn row_u128(row: &ParsedRow, key: &str) -> EvalResult<u128> {
    row_optional_u128(row, key)?.ok_or_else(|| format!("{} has null {key}", row.row_id))
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ParsedRefState {
    name: String,
    generation: u64,
    root: String,
}
pub(crate) fn row_ref(row: &ParsedRow, key: &str) -> EvalResult<ParsedRefState> {
    let object = json_object(&row.json, key)?;
    Ok(ParsedRefState {
        name: json_string(object, "name")?,
        generation: u64::try_from(json_u128(object, "generation")?).map_err(display_error)?,
        root: json_string(object, "root")?,
    })
}
pub(crate) fn validate_ref_chain(rows: &[ParsedRow], schedule: &FrozenSchedule) -> EvalResult<()> {
    let initial = rows
        .iter()
        .find(|row| row.row_group == "C02")
        .ok_or_else(|| "ref chain missing C02".to_owned())?;
    let mut previous = row_ref(initial, "post_ref")?;
    if previous.name != "main" || previous.generation != 1 {
        return Err("R0 RefState name=main; generation=1".to_owned());
    }
    let transitions = rows
        .iter()
        .filter(|row| matches!(row.row_group.as_str(), "C03" | "C05" | "C07"))
        .collect::<Vec<_>>();
    if transitions.len() != 34 {
        return Err(format!(
            "RefState transition rows {} != 34",
            transitions.len()
        ));
    }
    for (revision, row) in transitions.into_iter().enumerate() {
        let expected_revision = u8::try_from(revision + 1).map_err(display_error)?;
        if schedule.rows[row.schedule_index(schedule)?].transition_root != Some(expected_revision) {
            return Err(format!(
                "{} scheduled transition root R{expected_revision}",
                row.row_id
            ));
        }
        let pre = row_ref(row, "pre_ref")?;
        let post = row_ref(row, "post_ref")?;
        if pre != previous
            || pre.name != "main"
            || post.name != "main"
            || post.generation != pre.generation + 1
            || post.root == pre.root
        {
            return Err(format!(
                "{} RefState chain pre=previous; generation+1; name=main",
                row.row_id
            ));
        }
        previous = post;
    }
    Ok(())
}
impl ParsedRow {
    pub(crate) fn schedule_index(&self, schedule: &FrozenSchedule) -> EvalResult<usize> {
        schedule
            .rows
            .iter()
            .position(|row| row.row_id == self.row_id)
            .ok_or_else(|| format!("{} missing from schedule", self.row_id))
    }
}
