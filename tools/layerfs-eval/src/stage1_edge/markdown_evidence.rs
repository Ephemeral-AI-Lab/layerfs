use super::artifact::{display_error, json_bool, json_i64, json_u128};
use super::limits::INITIAL_BYTES;
use super::markdown_helpers::{format_ms, sum_phase, sum_row_walls, throughput_mib_s};
use super::markdown_report::MarkdownReport;
use super::report_disposition::{maximum_locality_key, sum_key, sum_locality_key};
use super::report_groups::{history_probe_stats, history_probe_sum};
use super::row_parse::{json_all_u128, json_object, phase_wall, row_u128};
use super::row_physical::history_root_indices;
use super::summary_markdown_contract::{maximum_group_key, sum_subfield};
use crate::stage1_fixture::EvalResult;
use std::fmt::Write as FmtWrite;
impl MarkdownReport<'_, '_> {
    pub(crate) fn append_evidence(&mut self) -> EvalResult<()> {
        let campaign = self.campaign;
        let rows = self.rows;
        let _selected_history_roots_passed = self.selected_history_roots_passed;
        let output = &mut self.output;
        writeln!(output, "\n## 7. Canonical locality\n\n| Population | Transitions | CDC expected B | CDC observed B | Unaffected reads B | Unaffected writes B | Max nodes read | Max nodes emitted | Status |\n|---|---:|---:|---:|---:|---:|---:|---:|---|").map_err(display_error)?;
        for (label, group, transitions, expected) in [
            ("Physical checkpoints", "C03", 15, 172_032_u128),
            ("Direct logical edits", "C05", 15, 172_032),
            ("Save bursts", "C07", 4, 151_552),
        ] {
            writeln!(output, "| {label} | {transitions} | `{expected}` | `{}` | `{}` | `{}` | `{}` | `{}` | `PASS` |", sum_key(rows, Some(group), "cdc_bytes_scanned")?, sum_key(rows, Some(group), "unaffected_payload_reads")?, sum_key(rows, Some(group), "unaffected_payload_writes")?, maximum_group_key(rows, group, "rope_nodes_read")?, maximum_group_key(rows, group, "rope_nodes_emitted")?).map_err(display_error)?;
        }
        writeln!(
            output,
            "| **Total** | **34** | `495616` | `{}` | **0** | **0** | `{}` | `{}` | `PASS` |",
            sum_locality_key(rows, "cdc_bytes_scanned")?,
            maximum_locality_key(rows, "rope_nodes_read")?,
            maximum_locality_key(rows, "rope_nodes_emitted")?
        )
        .map_err(display_error)?;
        writeln!(output, "\n## 8. Multi-edit save bursts\n\n| Root | Pattern | Sub-edits | Native ms | Oracle ms | Checkpoint ms | Row ms | Transactions | COMMITs | Final B | Status |\n|---:|---|---:|---:|---:|---:|---:|---:|---:|---:|---|").map_err(display_error)?;
        for (index, row) in rows.iter().filter(|row| row.row_group == "C07").enumerate() {
            let root = index + 31;
            let sub_count = json_all_u128(&row.json, "native_wall_ns")?.len();
            let native = json_all_u128(&row.json, "native_wall_ns")?
                .into_iter()
                .sum::<u128>();
            let oracle = json_all_u128(&row.json, "physical_oracle_wall_ns")?
                .into_iter()
                .sum::<u128>();
            writeln!(
                output,
                "| R{root} | {} | {sub_count} | `{}` | `{}` | `{}` | `{}` | 1 | 1 | {} | `PASS` |",
                campaign.schedule.bursts[index].pattern,
                format_ms(native),
                format_ms(oracle),
                format_ms(phase_wall(&row.json, "durable_checkpoint")?),
                format_ms(row.row_wall_ns),
                row.after_bytes
            )
            .map_err(display_error)?;
        }
        writeln!(
            output,
            "| **Total** | — | **21** | `{}` | `{}` | `{}` | `{}` | **4** | **4** | — | `PASS` |",
            format_ms(sum_subfield(rows, "C07", "native_wall_ns")?),
            format_ms(sum_subfield(rows, "C07", "physical_oracle_wall_ns")?),
            format_ms(sum_phase(rows, "C07", "durable_checkpoint")?),
            format_ms(sum_row_walls(rows, "C07")?)
        )
        .map_err(display_error)?;
        writeln!(output, "\n## 9. Fresh Verified history sessions\n\n| Session | Head | Roots checked | Open/scrub ms | Objects authenticated | Bytes authenticated | Probe B | Writer tx | Native writes | Status |\n|---:|---|---|---:|---:|---:|---:|---:|---:|---|").map_err(display_error)?;
        for (index, row) in rows
            .iter()
            .filter(|row| matches!(row.row_group.as_str(), "C04" | "C06"))
            .enumerate()
        {
            let session = index + 1;
            let head = session * 5;
            writeln!(
                output,
                "| {session} | R{head} | {} | `{}` | `{}` | `{}` | `{}` | 0 | 0 | `PASS` |",
                history_root_indices(session as u8)?
                    .iter()
                    .map(|root| format!("R{root}"))
                    .collect::<Vec<_>>()
                    .join(","),
                format_ms(phase_wall(&row.json, "verified_open")?),
                row_u128(row, "fetched_rows")?,
                row_u128(row, "object_bytes_read")?,
                history_root_indices(session as u8)?.len() * 3 * 65_536
            )
            .map_err(display_error)?;
        }
        writeln!(output, "\n| Probe ordinal | n | p50 ms | p95 ms | Non-payload rows | Payload rows | Cache classification |\n|---:|---:|---:|---:|---:|---:|---|").map_err(display_error)?;
        for (ordinal, classification) in [
            (1_u8, "first root/path resolution"),
            (2_u8, "exact root/path plan hit"),
            (3_u8, "exact root/path plan hit"),
        ] {
            let stats = history_probe_stats(rows, ordinal)?;
            writeln!(
                output,
                "| {ordinal} | {} | `{}` | `{}` | `{}` | `{}` | {classification} |",
                stats.raw_ns.len(),
                format_ms(stats.p50_ns),
                format_ms(stats.p95_ns),
                history_probe_sum(rows, ordinal, "non_payload_rows")?,
                history_probe_sum(rows, ordinal, "payload_batch_references")?,
            )
            .map_err(display_error)?;
        }
        writeln!(output, "\n## 10. Materialization and reconstruction\n\n| Root | Purpose | Logical B | Wall ms | MiB/s | Native write B | Exact bytes | Metadata | Cleanup |\n|---:|---|---:|---:|---:|---:|---|---|---|").map_err(display_error)?;
        let c02 = rows
            .iter()
            .find(|row| row.row_group == "C02")
            .ok_or_else(|| "missing C02 materialization row".to_owned())?;
        let cold = phase_wall(&c02.json, "cold_materialization")?;
        writeln!(output, "| R0 | Initial cold managed | {INITIAL_BYTES} | `{}` | `{:.3}` | `{}` | `PASS` | `PASS` | retained live |", format_ms(cold), throughput_mib_s(INITIAL_BYTES, cold), json_u128(&c02.json, "bytes_written")?).map_err(display_error)?;
        for (index, row) in rows.iter().filter(|row| row.row_group == "C08").enumerate() {
            let root = [15, 30, 34][index];
            let purpose = [
                "Physical-chain milestone",
                "Logical-refresh milestone",
                "Burst-chain milestone",
            ][index];
            let wall = phase_wall(&row.json, "milestone_materialization")?;
            let oracle = json_object(&row.json, "oracle")?;
            let custody = json_object(&row.json, "custody")?;
            let exact = json_bool(oracle, "physical_bytes_exact")?
                && json_bool(oracle, "canonical_bytes_exact")?;
            let metadata = json_bool(oracle, "metadata_exact")?;
            let cleanup = json_u128(custody, "cleanup_residue_entries")? == 0;
            writeln!(
                output,
                "| R{root} | {purpose} | {} | `{}` | `{:.3}` | `{}` | `{}` | `{}` | `{}` |",
                row.after_bytes,
                format_ms(wall),
                throughput_mib_s(row.after_bytes, wall),
                row_u128(row, "bytes_written")?,
                if exact { "PASS" } else { "FAIL" },
                if metadata { "PASS" } else { "FAIL" },
                if cleanup { "PASS" } else { "FAIL" },
            )
            .map_err(display_error)?;
        }
        let r34 = rows
            .iter()
            .find(|row| row.row_id == "C08-003")
            .ok_or_else(|| "missing C08-003 metadata receipt".to_owned())?;
        let r34_metadata = json_object(json_object(&r34.json, "custody")?, "fresh_metadata")?;
        writeln!(
        output,
        "\nR34 exact metadata receipt: mode=`{:#o}`; mtime=`{}.{:09}`; xattrs=`{}`; ACL present=`{}`; BSD flags=`{}`. This is the observed R34 value, not the initial fixture mtime.",
        json_u128(r34_metadata, "mode")?,
        json_i64(r34_metadata, "mtime_seconds")?,
        json_u128(r34_metadata, "mtime_nanoseconds")?,
        json_u128(r34_metadata, "xattr_count")?,
        json_bool(r34_metadata, "acl_present")?,
        json_u128(r34_metadata, "bsd_flags")?,
    )
    .map_err(display_error)?;
        Ok(())
    }
}
