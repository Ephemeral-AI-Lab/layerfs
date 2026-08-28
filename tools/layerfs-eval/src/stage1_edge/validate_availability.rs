use super::artifact::{json_bool, json_i64, json_string, json_u128};
use super::authentication::json_array_objects;
use super::limits::FIXTURE_MODE;
use super::row_parse::{json_object, row_u128, ParsedRow};
use super::summary_json_parse::json_top_level_value;
use crate::stage1_fixture::EvalResult;
pub(crate) fn validate_refresh_rows(rows: &[ParsedRow]) -> EvalResult<()> {
    let refreshes = rows
        .iter()
        .filter(|row| row.row_group == "C05")
        .collect::<Vec<_>>();
    if refreshes.len() != 15 {
        return Err(format!("logical refresh rows {} != 15", refreshes.len()));
    }
    let mut patches = 0_usize;
    let mut shifts = 0_usize;
    for row in refreshes {
        let edit = json_object(&row.json, "edit")?;
        let offset = json_u128(edit, "offset")?;
        let deleted = json_u128(edit, "delete_bytes")?;
        let inserted = json_u128(edit, "insert_bytes")?;
        if row_u128(row, "full_fallback_files")? != 0 {
            return Err(format!("{} unexpectedly used FullFallback", row.row_id));
        }
        if deleted == inserted {
            patches += 1;
            if !matches!(row.native_route.as_str(), "ClonePatch" | "InPlacePatch")
                || row_u128(row, "suffix_bytes_shifted")? != 0
                || row_u128(row, "patch_bytes")? != inserted
            {
                return Err(format!("{} retained patch route equation", row.row_id));
            }
            continue;
        }
        shifts += 1;
        let suffix = u128::from(row.before_bytes)
            .checked_sub(
                offset
                    .checked_add(deleted)
                    .ok_or_else(|| "refresh suffix overflow".to_owned())?,
            )
            .ok_or_else(|| "refresh suffix underflow".to_owned())?;
        if !matches!(row.native_route.as_str(), "CloneShift" | "InPlaceShift")
            || row_u128(row, "suffix_bytes_shifted")? != suffix
            || row_u128(row, "bytes_read")? != suffix
            || row_u128(row, "bytes_written")? != suffix + inserted
            || row_u128(row, "patch_bytes")? != inserted
        {
            return Err(format!("{} retained shift byte equation", row.row_id));
        }
        let attempts = row_u128(row, "clone_attempts")?;
        let successes = row_u128(row, "clone_successes")?;
        let fallbacks = row_u128(row, "clone_fallbacks")?;
        if row.native_route == "CloneShift" {
            if (attempts, successes, fallbacks) != (1, 1, 0) {
                return Err(format!("{} retained CloneShift equation", row.row_id));
            }
        } else if successes != 0 || attempts != fallbacks || attempts > 1 {
            return Err(format!("{} retained InPlaceShift equation", row.row_id));
        }
    }
    if patches != 3 || shifts != 12 {
        return Err(format!(
            "refresh route population {patches}/3 patch {shifts}/12 shift"
        ));
    }
    Ok(())
}
pub(crate) fn validate_availability_rows(rows: &[ParsedRow]) -> EvalResult<()> {
    for row in rows {
        let unavailable = json_array_objects(&row.json, "unavailable")?;
        let has_record = |field: &str| -> bool {
            unavailable.iter().any(|record| {
                json_string(record, "field").as_deref() == Ok(field)
                    && matches!(
                        json_string(record, "availability").as_deref(),
                        Ok("Unavailable" | "NotApplicable")
                    )
            })
        };
        for (object, fields) in [
            (
                "counters",
                &[
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
                    "payload_batch_maximum",
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
                    "scratch_high_water_bytes",
                    "retained_roots_validated",
                    "cdc_bytes_scanned",
                    "payload_bytes_written",
                    "unaffected_payload_reads",
                    "unaffected_payload_writes",
                    "rope_nodes_read",
                    "rope_nodes_emitted",
                    "content_directory_nodes_emitted",
                    "workspace_materializations",
                    "workspace_reuses",
                    "rematerializations",
                    "descriptor_resets",
                ][..],
            ),
            (
                "native",
                &[
                    "bytes_read",
                    "bytes_written",
                    "patch_bytes",
                    "suffix_bytes_shifted",
                    "clone_attempts",
                    "clone_successes",
                    "clone_fallbacks",
                    "full_fallback_files",
                    "files_created",
                    "files_replaced",
                    "files_removed",
                    "sync_regular_calls",
                    "sync_directory_calls",
                ],
            ),
            (
                "storage",
                &[
                    "database_bytes",
                    "logical_engine_bytes",
                    "rollback_journal_bytes",
                    "temporary_file_bytes",
                    "database_growth_bytes",
                    "canonical_object_bytes_written",
                    "physical_to_canonical_amplification",
                ],
            ),
            (
                "resources",
                &[
                    "rss_current_bytes",
                    "operation_q_current_bytes",
                    "operation_q_high_water_bytes",
                    "operation_q_terminal_bytes",
                    "owned_temp_entries",
                ],
            ),
            (
                "oracle",
                &[
                    "physical_bytes_exact",
                    "canonical_bytes_exact",
                    "metadata_exact",
                    "historical_roots_exact",
                    "route_exact",
                ],
            ),
        ] {
            let scoped = json_object(&row.json, object)?;
            for field in fields {
                if scoped.contains(&format!("\"{field}\":null"))
                    && !has_record(&format!("{object}.{field}"))
                {
                    return Err(format!(
                        "{} null {object}.{field} lacks availability record",
                        row.row_id
                    ));
                }
            }
        }
        if json_top_level_value(&row.json, "tree_level_before")?.starts_with("null")
            && !has_record("tree_level_before")
        {
            return Err(format!(
                "{} null tree_level_before lacks availability record",
                row.row_id
            ));
        }
    }
    Ok(())
}
pub(crate) fn validate_metadata_receipt(metadata: &str, label: &str) -> EvalResult<()> {
    let xattrs = json_array_objects(metadata, "xattrs")?;
    let acl_present = json_bool(metadata, "acl_present")?;
    json_i64(metadata, "mtime_seconds")?;
    if json_u128(metadata, "mode")? != u128::from(FIXTURE_MODE)
        || json_u128(metadata, "mtime_nanoseconds")? >= 1_000_000_000
        || json_u128(metadata, "xattr_count")? != xattrs.len() as u128
        || !xattrs.is_empty()
        || acl_present
        || !metadata.contains("\"acl_hex\":null")
        || json_u128(metadata, "bsd_flags")? != 0
    {
        return Err(format!("{label} exact supported metadata receipt"));
    }
    Ok(())
}
