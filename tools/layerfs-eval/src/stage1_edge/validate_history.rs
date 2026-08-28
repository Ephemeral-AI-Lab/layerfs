use super::artifact::{display_error, json_bool, json_string, json_u128};
use super::authentication::json_array_objects;
use super::row_parse::{json_object, phase_wall, ParsedRow};
use super::row_physical::{history_custody_json, history_root_indices};
use super::schedule::{frozen_schedule, oracle_snapshots};
use super::validate_availability::validate_metadata_receipt;
use crate::stage1_fixture::EvalResult;
pub(crate) fn validate_history_rows(rows: &[ParsedRow]) -> EvalResult<usize> {
    let root_rows = rows
        .iter()
        .filter(|row| matches!(row.row_group.as_str(), "C02" | "C03" | "C05" | "C07"))
        .collect::<Vec<_>>();
    if root_rows.len() != 35 {
        return Err(format!(
            "retained root digest rows {} != 35",
            root_rows.len()
        ));
    }
    let root_digests = root_rows
        .iter()
        .map(|row| json_string(json_object(&row.json, "oracle")?, "content_digest"))
        .collect::<EvalResult<Vec<_>>>()?;
    if root_digests
        .iter()
        .any(|digest| digest.len() != 64 || !digest.bytes().all(|byte| byte.is_ascii_hexdigit()))
    {
        return Err("retained root digest custody".to_owned());
    }
    let schedule = frozen_schedule()?;
    let snapshots = oracle_snapshots(&schedule)?;
    let sessions = rows
        .iter()
        .filter(|row| matches!(row.row_group.as_str(), "C04" | "C06"))
        .collect::<Vec<_>>();
    if sessions.len() != 6 {
        return Err(format!("retained history sessions {} != 6", sessions.len()));
    }
    let mut selected = Vec::new();
    let mut probe_count = 0_usize;
    for (index, row) in sessions.into_iter().enumerate() {
        let session = u8::try_from(index + 1).map_err(display_error)?;
        if json_object(&row.json, "custody")? != history_custody_json(session)?
            || !json_bool(json_object(&row.json, "oracle")?, "historical_roots_exact")?
            || json_string(json_object(&row.json, "oracle")?, "content_digest")?
                != root_digests[usize::from(session) * 5]
        {
            return Err(format!(
                "{} exact retained history-root receipt",
                row.row_id
            ));
        }
        let phase = json_array_objects(&row.json, "phase_counters")?
            .into_iter()
            .find(|phase| json_string(phase, "name").as_deref() == Ok("history_read"))
            .ok_or_else(|| format!("{} missing history_read phase counters", row.row_id))?;
        let probes = json_array_objects(&row.json, "history_probes")?;
        let expected_roots = history_root_indices(session)?;
        if probes.len() != expected_roots.len() * 3 {
            return Err(format!("{} exact history probe population", row.row_id));
        }
        let mut wall_sum = 0_u128;
        for (probe_index, probe) in probes.iter().enumerate() {
            let root_index = expected_roots[probe_index / 3];
            let ordinal = probe_index % 3 + 1;
            let logical_length = snapshots[root_index].logical_length;
            let start = match ordinal {
                1 => 0,
                2 => logical_length / 2 - 32_768,
                3 => logical_length - 65_536,
                _ => unreachable!(),
            };
            let counters = json_object(probe, "engine_counters")?;
            let fetched = json_u128(probe, "fetched_rows")?;
            let payload_references = json_u128(probe, "payload_batch_references")?;
            let statements = json_u128(counters, "statements")?;
            let payload_queries = json_u128(probe, "payload_batch_queries")?;
            if json_string(probe, "root")? != format!("R{root_index}")
                || json_u128(probe, "ordinal")? != ordinal as u128
                || json_u128(probe, "start")? != u128::from(start)
                || json_u128(probe, "length")? != 65_536
                || json_u128(probe, "payload_bytes_read")? != 65_536
                || fetched != json_u128(probe, "authentication_passes")?
                || fetched != json_u128(probe, "role_decode_passes")?
                || json_u128(probe, "non_payload_rows")?
                    != fetched.checked_sub(payload_references).ok_or_else(|| {
                        format!("{} probe payload rows exceed fetched rows", row.row_id)
                    })?
                || json_u128(probe, "non_payload_statements")?
                    != statements.checked_sub(payload_queries).ok_or_else(|| {
                        format!("{} probe payload queries exceed statements", row.row_id)
                    })?
                || (ordinal == 1
                    && (json_u128(probe, "namespace_nodes_read")? == 0
                        || json_u128(probe, "inode_table_nodes_read")? == 0))
                || (ordinal != 1
                    && (json_u128(probe, "namespace_nodes_read")? != 0
                        || json_u128(probe, "inode_table_nodes_read")? != 0))
                || json_u128(counters, "transactions_started")? != 0
                || json_u128(counters, "transactions_committed")? != 0
                || json_u128(counters, "publication_transactions_started")? != 0
                || json_u128(counters, "publication_transactions_rolled_back")? != 0
                || json_u128(counters, "publication_commits")? != 0
                || json_u128(counters, "object_bytes_written")? != 0
                || json_u128(counters, "cdc_bytes_scanned")? != 0
                || json_u128(counters, "payload_bytes_written")? != 0
            {
                return Err(format!("{} exact ordered history probe", row.row_id));
            }
            wall_sum = wall_sum
                .checked_add(json_u128(probe, "wall_ns")?)
                .ok_or_else(|| format!("{} probe wall overflow", row.row_id))?;
        }
        if wall_sum > phase_wall(&row.json, "history_read")? {
            return Err(format!("{} probe walls exceed history phase", row.row_id));
        }
        for key in [
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
            "scratch_tables",
            "scratch_statements",
            "scratch_rows",
            "retained_roots_validated",
        ] {
            let sum = probes.iter().try_fold(0_u128, |sum, probe| {
                sum.checked_add(json_u128(json_object(probe, "engine_counters")?, key)?)
                    .ok_or_else(|| format!("{} probe {key} overflow", row.row_id))
            })?;
            if sum != json_u128(phase, key)? {
                return Err(format!("{} probe {key} sum", row.row_id));
            }
        }
        let payload_max = probes
            .iter()
            .map(|probe| {
                json_u128(
                    json_object(probe, "engine_counters")?,
                    "payload_batch_maximum",
                )
            })
            .collect::<EvalResult<Vec<_>>>()?
            .into_iter()
            .max()
            .unwrap_or(0);
        if payload_max != json_u128(phase, "payload_batch_maximum")? {
            return Err(format!("{} probe payload batch maximum", row.row_id));
        }
        probe_count += probes.len();
        selected.extend_from_slice(history_root_indices(session)?);
    }
    if probe_count != 63 {
        return Err(format!("retained history probes {probe_count} != 63"));
    }
    selected.sort_unstable();
    selected.dedup();
    if selected != [0, 5, 10, 15, 20, 25, 30] {
        return Err(format!("retained selected history roots {selected:?}"));
    }
    let milestones = rows
        .iter()
        .filter(|row| row.row_group == "C08")
        .collect::<Vec<_>>();
    if milestones.len() != 3 {
        return Err(format!("retained milestone rows {} != 3", milestones.len()));
    }
    for (row, root) in milestones.into_iter().zip([15_u8, 30, 34]) {
        let oracle = json_object(&row.json, "oracle")?;
        let custody = json_object(&row.json, "custody")?;
        let metadata = json_object(custody, "metadata")?;
        let retained_metadata = json_object(custody, "retained_metadata")?;
        let fresh_metadata = json_object(custody, "fresh_metadata")?;
        if json_string(custody, "milestone_root")? != format!("R{root}")
            || json_u128(custody, "extra_user_files")? != 0
            || json_u128(custody, "fresh_extra_user_files")? != 0
            || json_u128(custody, "cleanup_residue_entries")? != 0
            || metadata != fresh_metadata
            || fresh_metadata != retained_metadata
            || !json_bool(oracle, "physical_bytes_exact")?
            || !json_bool(oracle, "canonical_bytes_exact")?
            || !json_bool(oracle, "metadata_exact")?
            || !json_bool(oracle, "historical_roots_exact")?
        {
            return Err(format!("{} exact milestone/history receipt", row.row_id));
        }
        validate_metadata_receipt(fresh_metadata, &format!("R{root}"))?;
        if root == 34 {
            if json_u128(custody, "live_extra_user_files")? != 0
                || json_object(custody, "live_metadata")? != fresh_metadata
            {
                return Err("R34 live/fresh tree and metadata receipt".to_owned());
            }
        } else if !custody.contains("\"live_extra_user_files\":null")
            || !custody.contains("\"live_metadata\":null")
        {
            return Err(format!("R{root} live custody is not applicable"));
        }
    }
    Ok(selected.len() + 1)
}
