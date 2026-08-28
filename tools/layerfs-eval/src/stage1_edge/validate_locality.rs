use super::artifact::{display_error, json_string, json_u128};
use super::authentication::json_array_objects;
use super::locality::ContentCounters;
use super::row_parse::{json_object, row_u128, ParsedRow};
use super::summary_json_parse::json_object_member_names;
use crate::stage1_fixture::EvalResult;
pub(crate) fn validate_locality_rows(rows: &[ParsedRow]) -> EvalResult<()> {
    let individual = rows
        .iter()
        .filter(|row| matches!(row.row_group.as_str(), "C03" | "C05"))
        .collect::<Vec<_>>();
    if individual.len() != 30 {
        return Err(format!(
            "individual locality rows {} != 30",
            individual.len()
        ));
    }
    for row in individual {
        let tree_level =
            u8::try_from(json_u128(&row.json, "tree_level_before")?).map_err(display_error)?;
        let replacement = json_u128(json_object(&row.json, "edit")?, "insert_bytes")?;
        let read_bound = 16_u128 * (u128::from(tree_level) + 1);
        let emitted_bound = read_bound + replacement.div_ceil(8_192) + 2;
        if row_u128(row, "cdc_bytes_scanned")? != replacement
            || row_u128(row, "payload_bytes_written")? != replacement
            || row_u128(row, "unaffected_payload_reads")? != 0
            || row_u128(row, "unaffected_payload_writes")? != 0
            || row_u128(row, "content_directory_nodes_emitted")? != 0
            || row_u128(row, "rope_nodes_read")? > read_bound
            || row_u128(row, "rope_nodes_emitted")? > emitted_bound
        {
            return Err(format!("{} retained locality/H equation", row.row_id));
        }
    }
    let mut subedit_count = 0_usize;
    for row in rows.iter().filter(|row| row.row_group == "C07") {
        let mut exact = ContentCounters::default();
        let mut native_exact = [0_u128; 8];
        for subedit in json_array_objects(&row.json, "sub_edits")? {
            subedit_count += 1;
            let required = [
                "tag",
                "offset",
                "delete_bytes",
                "insert_bytes",
                "replacement_digest",
                "before_bytes",
                "after_bytes",
                "native_wall_ns",
                "physical_oracle_wall_ns",
                "native_route",
                "native_bytes_read",
                "native_bytes_written",
                "native_patch_bytes",
                "native_suffix_bytes_shifted",
                "native_clone_attempts",
                "native_clone_successes",
                "native_clone_fallbacks",
                "native_full_fallback_files",
                "tree_level_before",
                "cdc_bytes_scanned",
                "payload_bytes_written",
                "unaffected_payload_reads",
                "unaffected_payload_writes",
                "rope_nodes_read",
                "rope_nodes_emitted",
                "content_directory_nodes_emitted",
            ];
            if json_object_member_names(subedit)? != required {
                return Err(format!("{} exact flattened sub-edit schema", row.row_id));
            }
            let replacement = json_u128(subedit, "insert_bytes")?;
            let delete = json_u128(subedit, "delete_bytes")?;
            let before = json_u128(subedit, "before_bytes")?;
            let offset = json_u128(subedit, "offset")?;
            let route = json_string(subedit, "native_route")?;
            let native_read = json_u128(subedit, "native_bytes_read")?;
            let native_written = json_u128(subedit, "native_bytes_written")?;
            let native_patch = json_u128(subedit, "native_patch_bytes")?;
            let native_suffix = json_u128(subedit, "native_suffix_bytes_shifted")?;
            let clone_attempts = json_u128(subedit, "native_clone_attempts")?;
            let clone_successes = json_u128(subedit, "native_clone_successes")?;
            let clone_fallbacks = json_u128(subedit, "native_clone_fallbacks")?;
            let full_fallbacks = json_u128(subedit, "native_full_fallback_files")?;
            for (total, value) in native_exact.iter_mut().zip([
                native_read,
                native_written,
                native_patch,
                native_suffix,
                clone_attempts,
                clone_successes,
                clone_fallbacks,
                full_fallbacks,
            ]) {
                *total = total
                    .checked_add(value)
                    .ok_or_else(|| format!("{} native sub-edit sum overflow", row.row_id))?;
            }
            if delete == replacement {
                if !matches!(route.as_str(), "ClonePatch" | "InPlacePatch")
                    || native_read != 0
                    || native_written != replacement
                    || native_patch != replacement
                    || native_suffix != 0
                    || clone_attempts != 1
                    || (route == "ClonePatch" && (clone_successes != 1 || clone_fallbacks != 0))
                    || (route == "InPlacePatch" && (clone_successes != 0 || clone_fallbacks != 1))
                    || full_fallbacks != 0
                {
                    return Err(format!(
                        "{} exact sub-edit native patch equation",
                        row.row_id
                    ));
                }
            } else {
                let suffix = before
                    .checked_sub(
                        offset
                            .checked_add(delete)
                            .ok_or_else(|| "sub-edit native suffix overflow".to_owned())?,
                    )
                    .ok_or_else(|| "sub-edit native suffix underflow".to_owned())?;
                if route != "InPlaceShift"
                    || native_read != suffix
                    || native_written != suffix + replacement
                    || native_patch != replacement
                    || native_suffix != suffix
                    || clone_attempts != 0
                    || clone_successes != 0
                    || clone_fallbacks != 0
                    || full_fallbacks != 0
                {
                    return Err(format!(
                        "{} exact sub-edit native shift equation",
                        row.row_id
                    ));
                }
            }
            let tree_level = json_u128(subedit, "tree_level_before")?;
            let read_bound = 16 * (tree_level + 1);
            let emitted_bound = read_bound + replacement.div_ceil(8_192) + 2;
            let cdc = json_u128(subedit, "cdc_bytes_scanned")?;
            let payload = json_u128(subedit, "payload_bytes_written")?;
            let unaffected_reads = json_u128(subedit, "unaffected_payload_reads")?;
            let unaffected_writes = json_u128(subedit, "unaffected_payload_writes")?;
            let nodes_read = json_u128(subedit, "rope_nodes_read")?;
            let nodes_emitted = json_u128(subedit, "rope_nodes_emitted")?;
            let directory_nodes = json_u128(subedit, "content_directory_nodes_emitted")?;
            if cdc != replacement
                || payload != replacement
                || unaffected_reads != 0
                || unaffected_writes != 0
                || nodes_read > read_bound
                || nodes_emitted > emitted_bound
                || directory_nodes != 0
            {
                return Err(format!(
                    "{} retained sub-edit locality/H equation",
                    row.row_id
                ));
            }
            exact.cdc_bytes_scanned += u64::try_from(cdc).map_err(display_error)?;
            exact.payload_bytes_written += u64::try_from(payload).map_err(display_error)?;
            exact.unaffected_payload_reads +=
                u64::try_from(unaffected_reads).map_err(display_error)?;
            exact.unaffected_payload_writes +=
                u64::try_from(unaffected_writes).map_err(display_error)?;
            exact.rope_nodes_read += u64::try_from(nodes_read).map_err(display_error)?;
            exact.rope_nodes_emitted += u64::try_from(nodes_emitted).map_err(display_error)?;
            exact.content_directory_nodes_emitted +=
                u64::try_from(directory_nodes).map_err(display_error)?;
        }
        if row_u128(row, "cdc_bytes_scanned")? != u128::from(exact.cdc_bytes_scanned)
            || row_u128(row, "payload_bytes_written")? != u128::from(exact.payload_bytes_written)
            || row_u128(row, "unaffected_payload_reads")?
                != u128::from(exact.unaffected_payload_reads)
            || row_u128(row, "unaffected_payload_writes")?
                != u128::from(exact.unaffected_payload_writes)
            || row_u128(row, "rope_nodes_read")? != u128::from(exact.rope_nodes_read)
            || row_u128(row, "rope_nodes_emitted")? != u128::from(exact.rope_nodes_emitted)
            || row_u128(row, "content_directory_nodes_emitted")?
                != u128::from(exact.content_directory_nodes_emitted)
        {
            return Err(format!("{} retained exact sub-edit aggregate", row.row_id));
        }
        let native = json_object(&row.json, "native")?;
        for (key, expected) in [
            "bytes_read",
            "bytes_written",
            "patch_bytes",
            "suffix_bytes_shifted",
            "clone_attempts",
            "clone_successes",
            "clone_fallbacks",
            "full_fallback_files",
        ]
        .into_iter()
        .zip(native_exact)
        {
            if json_u128(native, key)? != expected {
                return Err(format!("{} native {key} sub-edit aggregate", row.row_id));
            }
        }
    }
    if subedit_count != 21 {
        return Err(format!(
            "retained sub-edit locality rows {subedit_count} != 21"
        ));
    }
    Ok(())
}
pub(crate) fn validate_phase_counter_rows(rows: &[ParsedRow]) -> EvalResult<()> {
    const ADDITIVE: &[&str] = &[
        "transactions_started",
        "transactions_committed",
        "transactions_rolled_back",
        "statements",
        "admission_transactions_started",
        "admission_transactions_committed",
        "admission_transactions_rolled_back",
        "admission_statements",
        "integrity_transactions_started",
        "integrity_transactions_committed",
        "integrity_transactions_rolled_back",
        "integrity_statements",
        "busy_events",
        "locked_events",
        "objects_validated",
        "objects_created",
        "objects_reused",
        "object_bytes_read",
        "object_bytes_written",
        "fetched_rows",
        "fetched_row_authentication_passes",
        "fetched_row_role_decode_passes",
        "new_object_authentication_passes",
        "incumbent_authentication_passes",
        "payload_batch_queries",
        "payload_batch_references",
        "put_lookup_statements",
        "put_insert_statements",
        "created_rows",
        "reused_rows",
        "publication_transactions_started",
        "publication_transactions_rolled_back",
        "publication_commits",
        "publication_closure_passes",
        "namespace_graph_verification_passes",
        "retained_roots_validated",
    ];
    for row in rows {
        let expected: &[&str] = match row.row_group.as_str() {
            "C02" => &[
                "store_open",
                "storage_observation",
                "materialization",
                "storage_observation",
            ],
            "C03" | "C07" => &[
                "native_edit",
                "checkpoint",
                "canonical_witness",
                "storage_observation",
            ],
            "C04" | "C06" => &[
                "verified_open",
                "storage_observation",
                "history_read",
                "storage_observation",
            ],
            "C05" => &[
                "logical_edit",
                "apfs_refresh",
                "canonical_witness",
                "storage_observation",
            ],
            "C08" => &[
                "verified_open",
                "storage_observation",
                "materialization",
                "storage_observation",
            ],
            "C09" => &["explicit_cleanup"],
            _ => &[],
        };
        let phases = json_array_objects(&row.json, "phase_counters")?;
        let names = phases
            .iter()
            .map(|phase| json_string(phase, "name"))
            .collect::<EvalResult<Vec<_>>>()?;
        if names != expected {
            return Err(format!(
                "{} phase counter names {names:?} != {expected:?}",
                row.row_id
            ));
        }
        if expected.is_empty() {
            continue;
        }
        for phase in &phases {
            let fetched = json_u128(phase, "fetched_rows")?;
            let authenticated = json_u128(phase, "fetched_row_authentication_passes")?;
            let decoded = json_u128(phase, "fetched_row_role_decode_passes")?;
            let created = json_u128(phase, "created_rows")?;
            let reused = json_u128(phase, "reused_rows")?;
            let new = json_u128(phase, "new_object_authentication_passes")?;
            let incumbent = json_u128(phase, "incumbent_authentication_passes")?;
            let retained_scrubs = json_u128(phase, "retained_union_scrubs")?;
            let retained_roots = json_u128(phase, "retained_roots_validated")?;
            let namespace_graphs = json_u128(phase, "namespace_graph_verification_passes")?;
            let name = json_string(phase, "name")?;
            let trusted = matches!(row.row_group.as_str(), "C02" | "C03" | "C05" | "C07");
            let trusted_read_only = row.row_group == "C02"
                || matches!(name.as_str(), "canonical_witness" | "apfs_refresh");
            if ((trusted_read_only && authenticated != 0)
                || (trusted && authenticated > fetched)
                || (!trusted && fetched != authenticated))
                || fetched != decoded
                || new != created + reused
                || new != json_u128(phase, "put_lookup_statements")?
                || incumbent != reused
                || json_u128(phase, "put_insert_statements")? != created
                || json_u128(phase, "objects_created")? != created
                || json_u128(phase, "objects_reused")? != reused
                || json_u128(phase, "objects_validated")? != decoded + new + incumbent
                || json_u128(phase, "object_bytes_written")?
                    != json_u128(phase, "logical_object_bytes")?
                || json_u128(phase, "range_bytes_requested")?
                    != json_u128(phase, "range_bytes_returned")?
                || json_u128(phase, "payload_batch_maximum")? > 64
                || json_u128(phase, "admission_transactions_started")?
                    != json_u128(phase, "admission_transactions_committed")?
                        + json_u128(phase, "admission_transactions_rolled_back")?
                || json_u128(phase, "integrity_transactions_started")?
                    != json_u128(phase, "integrity_transactions_committed")?
                        + json_u128(phase, "integrity_transactions_rolled_back")?
                || json_u128(phase, "publication_transactions_started")?
                    != json_u128(phase, "publication_commits")?
                        + json_u128(phase, "publication_transactions_rolled_back")?
                || (retained_scrubs != 0 && retained_roots != namespace_graphs)
                || json_u128(phase, "q_before_bytes")? != 0
                || json_u128(phase, "q_after_bytes")? != 0
                || json_u128(phase, "q_high_water_bytes")? > 8_388_608
                || json_u128(phase, "active_connections")? > 2
            {
                return Err(format!("{} phase counter equation", row.row_id));
            }
        }
        for key in ADDITIVE {
            let phase_sum = phases.iter().try_fold(0_u128, |total, phase| {
                total
                    .checked_add(json_u128(phase, key)?)
                    .ok_or_else(|| format!("{} phase {key} sum overflow", row.row_id))
            })?;
            if phase_sum != row_u128(row, key)? {
                return Err(format!(
                    "{} phase {key} sum {phase_sum} != row {}",
                    row.row_id,
                    row_u128(row, key)?
                ));
            }
        }
        {
            let key = "payload_batch_maximum";
            let phase_max = phases
                .iter()
                .map(|phase| json_u128(phase, key))
                .collect::<EvalResult<Vec<_>>>()?
                .into_iter()
                .max()
                .unwrap_or(0);
            if phase_max != row_u128(row, key)? {
                return Err(format!("{} phase {key} maximum", row.row_id));
            }
        }
        for (engine_key, operation_key) in [
            ("scratch_tables", "operation_scratch_tables"),
            ("scratch_statements", "operation_scratch_statements"),
            ("scratch_rows", "operation_scratch_rows"),
        ] {
            let engine = phases.iter().try_fold(0_u128, |total, phase| {
                total
                    .checked_add(json_u128(phase, engine_key)?)
                    .ok_or_else(|| format!("{} phase {engine_key} overflow", row.row_id))
            })?;
            let operation = phases.iter().try_fold(0_u128, |total, phase| {
                total
                    .checked_add(json_u128(phase, operation_key)?)
                    .ok_or_else(|| format!("{} phase {operation_key} overflow", row.row_id))
            })?;
            let combined = engine
                .checked_add(operation)
                .ok_or_else(|| format!("{} combined {engine_key} overflow", row.row_id))?;
            if combined != row_u128(row, engine_key)? {
                return Err(format!(
                    "{} phase Engine/VFS {engine_key} aggregate",
                    row.row_id
                ));
            }
        }
        let engine_scratch_high = phases
            .iter()
            .map(|phase| json_u128(phase, "scratch_high_water_bytes"))
            .collect::<EvalResult<Vec<_>>>()?
            .into_iter()
            .max()
            .unwrap_or(0);
        let operation_scratch_high = phases
            .iter()
            .map(|phase| json_u128(phase, "operation_scratch_high_water_bytes"))
            .collect::<EvalResult<Vec<_>>>()?
            .into_iter()
            .max()
            .unwrap_or(0);
        if engine_scratch_high.max(operation_scratch_high)
            != row_u128(row, "scratch_high_water_bytes")?
        {
            return Err(format!(
                "{} phase Engine/VFS scratch_high_water_bytes aggregate",
                row.row_id
            ));
        }
    }
    Ok(())
}
