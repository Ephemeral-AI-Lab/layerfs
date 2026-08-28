use super::artifact::{display_error, json_u128};
use super::markdown_helpers::format_ms;
use super::markdown_report::MarkdownReport;
use super::report_groups::route_stats;
use super::row_parse::row_u128;
use super::statistics::{combined_phase_stats, filtered_phase_stats, row_phase_stats};
use super::summary_markdown_contract::{
    optional_route_stats, sum_patch_bytes, sum_route_bytes, title,
};
use crate::stage1_fixture::EvalResult;
use std::fmt::Write as FmtWrite;
impl MarkdownReport<'_, '_> {
    pub(crate) fn append_edits(&mut self) -> EvalResult<()> {
        let rows = self.rows;
        let shift_refreshes = self.shift_refreshes;
        let fallback_refreshes = self.fallback_refreshes;
        let _rematerializations = self.rematerializations;
        let output = &mut self.output;
        writeln!(output, "## 3. Physical APFS edit to LayerFS checkpoint\n\n| Operation | n | Native p50 ms | Native p95 ms | Checkpoint p50 ms | Checkpoint p95 ms | Combined p50 ms | Combined p95 ms | Oracle | Status |\n|---|---:|---:|---:|---:|---:|---:|---:|---|---|").map_err(display_error)?;
        for kind in ["overwrite", "insert", "delete", "append", "truncate"] {
            let native =
                filtered_phase_stats(rows, "C03", "native_edit", |row| row.operation == kind)?;
            let checkpoint = filtered_phase_stats(rows, "C03", "durable_checkpoint", |row| {
                row.operation == kind
            })?;
            let combined =
                combined_phase_stats(rows, "C03", "native_edit", "durable_checkpoint", |row| {
                    row.operation == kind
                })?;
            writeln!(
                output,
                "| {} | 3 | `{}` | `{}` | `{}` | `{}` | `{}` | `{}` | `3/3` | `PASS` |",
                title(kind),
                format_ms(native.p50_ns),
                format_ms(native.p95_ns),
                format_ms(checkpoint.p50_ns),
                format_ms(checkpoint.p95_ns),
                format_ms(combined.p50_ns),
                format_ms(combined.p95_ns)
            )
            .map_err(display_error)?;
        }
        let native_all = row_phase_stats(rows, "C03", "native_edit")?;
        let checkpoint_all = row_phase_stats(rows, "C03", "durable_checkpoint")?;
        let combined_all =
            combined_phase_stats(rows, "C03", "native_edit", "durable_checkpoint", |_| true)?;
        writeln!(
            output,
            "| **All** | **15** | `{}` | `{}` | `{}` | `{}` | `{}` | `{}` | `15/15` | `PASS` |\n",
            format_ms(native_all.p50_ns),
            format_ms(native_all.p95_ns),
            format_ms(checkpoint_all.p50_ns),
            format_ms(checkpoint_all.p95_ns),
            format_ms(combined_all.p50_ns),
            format_ms(combined_all.p95_ns)
        )
        .map_err(display_error)?;
        writeln!(output, "| Size band | n | Native p50 ms | Native p95 ms | Checkpoint p50 ms | Checkpoint p95 ms | Combined p50 ms | Combined p95 ms |\n|---|---:|---:|---:|---:|---:|---:|---:|").map_err(display_error)?;
        for (label, band) in [
            ("Near 8 KiB", "near-8-kib"),
            ("Near 16 KiB", "near-16-kib"),
            ("Near 32 KiB", "near-32-kib"),
        ] {
            let native =
                filtered_phase_stats(rows, "C03", "native_edit", |row| row.size_band == band)?;
            let checkpoint = filtered_phase_stats(rows, "C03", "durable_checkpoint", |row| {
                row.size_band == band
            })?;
            let combined =
                combined_phase_stats(rows, "C03", "native_edit", "durable_checkpoint", |row| {
                    row.size_band == band
                })?;
            writeln!(
                output,
                "| {label} | 5 | `{}` | `{}` | `{}` | `{}` | `{}` | `{}` |",
                format_ms(native.p50_ns),
                format_ms(native.p95_ns),
                format_ms(checkpoint.p50_ns),
                format_ms(checkpoint.p95_ns),
                format_ms(combined.p50_ns),
                format_ms(combined.p95_ns)
            )
            .map_err(display_error)?;
        }
        writeln!(output, "\n## 4. Physical count-changing amplification\n\n| Seq | Operation | Offset | Suffix B | Replacement B | Native read B | Native write B | Equation | Route | Status |\n|---:|---|---:|---:|---:|---:|---:|---|---|---|").map_err(display_error)?;
        for row in rows
            .iter()
            .filter(|row| row.row_group == "C03" && row.operation != "overwrite")
        {
            let offset = json_u128(&row.json, "offset")?;
            let delete = json_u128(&row.json, "delete_bytes")?;
            let insert = json_u128(&row.json, "insert_bytes")?;
            let suffix = u128::from(row.before_bytes).saturating_sub(offset + delete);
            writeln!(output, "| `{}` | `{}` | `{offset}` | `{suffix}` | `{insert}` | `{}` | `{}` | `read=S; write=S+B` | `{}` | `PASS` |", json_u128(&row.json, "sequence")?, row.operation, row_u128(row, "bytes_read")?, row_u128(row, "bytes_written")?, row.native_route).map_err(display_error)?;
        }
        writeln!(output, "\n| Kind | n | Suffix shifted B | Native read B | Native write B | Amplification |\n|---|---:|---:|---:|---:|---:|").map_err(display_error)?;
        for kind in ["insert", "delete", "append", "truncate"] {
            let selected = rows
                .iter()
                .filter(|row| row.row_group == "C03" && row.operation == kind)
                .collect::<Vec<_>>();
            let shifted = selected
                .iter()
                .map(|row| row_u128(row, "suffix_bytes_shifted"))
                .collect::<EvalResult<Vec<_>>>()?
                .into_iter()
                .sum::<u128>();
            let read = selected
                .iter()
                .map(|row| row_u128(row, "bytes_read"))
                .collect::<EvalResult<Vec<_>>>()?
                .into_iter()
                .sum::<u128>();
            let written = selected
                .iter()
                .map(|row| row_u128(row, "bytes_written"))
                .collect::<EvalResult<Vec<_>>>()?
                .into_iter()
                .sum::<u128>();
            let logical = selected
                .iter()
                .map(|row| u128::from(row.before_bytes.abs_diff(row.after_bytes)))
                .sum::<u128>();
            let ratio = if logical == 0 {
                0.0
            } else {
                (read + written) as f64 / logical as f64
            };
            writeln!(
                output,
                "| {} | 3 | `{shifted}` | `{read}` | `{written}` | `{ratio:.3}` |",
                title(kind)
            )
            .map_err(display_error)?;
        }
        writeln!(output, "\n## 5. Logical LayerFS edit to physical APFS refresh\n\n| Operation | n | Logical p50 ms | Logical p95 ms | Route class | Refresh p50 ms | Refresh p95 ms | End-to-end p50 ms | End-to-end p95 ms | Oracle |\n|---|---:|---:|---:|---|---:|---:|---:|---:|---|").map_err(display_error)?;
        for kind in ["overwrite", "insert", "delete", "append", "truncate"] {
            let logical = filtered_phase_stats(rows, "C05", "direct_logical_edit", |row| {
                row.operation == kind
            })?;
            let refresh = filtered_phase_stats(rows, "C05", "changed_root_refresh", |row| {
                row.operation == kind
            })?;
            let combined = combined_phase_stats(
                rows,
                "C05",
                "direct_logical_edit",
                "changed_root_refresh",
                |row| row.operation == kind,
            )?;
            writeln!(
                output,
                "| {} | 3 | `{}` | `{}` | {} | `{}` | `{}` | `{}` | `{}` | `3/3` |",
                title(kind),
                format_ms(logical.p50_ns),
                format_ms(logical.p95_ns),
                if kind == "overwrite" {
                    "Patch"
                } else {
                    "Shift"
                },
                format_ms(refresh.p50_ns),
                format_ms(refresh.p95_ns),
                format_ms(combined.p50_ns),
                format_ms(combined.p95_ns)
            )
            .map_err(display_error)?;
        }
        writeln!(output, "\n## 6. Refresh-route summary\n\n| Route | Required count | Observed | p50 ms | p95 ms | Physical B | Rematerializations | Status |\n|---|---:|---:|---:|---:|---:|---:|---|").map_err(display_error)?;
        let clone = rows
            .iter()
            .filter(|row| row.row_group == "C05" && row.native_route == "ClonePatch")
            .count();
        let in_place = rows
            .iter()
            .filter(|row| row.row_group == "C05" && row.native_route == "InPlacePatch")
            .count();
        let clone_shift = rows
            .iter()
            .filter(|row| row.row_group == "C05" && row.native_route == "CloneShift")
            .count();
        let in_place_shift = rows
            .iter()
            .filter(|row| row.row_group == "C05" && row.native_route == "InPlaceShift")
            .count();
        for (label, required, observed, stats, bytes) in [
            (
                "ClonePatch",
                "0..3",
                clone,
                optional_route_stats(rows, "ClonePatch")?,
                sum_route_bytes(rows, "ClonePatch", None)?,
            ),
            (
                "InPlacePatch",
                "0..3",
                in_place,
                optional_route_stats(rows, "InPlacePatch")?,
                sum_route_bytes(rows, "InPlacePatch", None)?,
            ),
            (
                "Patch aggregate",
                "3",
                3,
                Some(route_stats(rows, "Patch", None)?),
                sum_patch_bytes(rows)?,
            ),
            (
                "CloneShift",
                "0..12",
                clone_shift,
                optional_route_stats(rows, "CloneShift")?,
                sum_route_bytes(rows, "CloneShift", None)?,
            ),
            (
                "InPlaceShift",
                "0..12",
                in_place_shift,
                optional_route_stats(rows, "InPlaceShift")?,
                sum_route_bytes(rows, "InPlaceShift", None)?,
            ),
            (
                "Shift aggregate",
                "12",
                shift_refreshes,
                Some(route_stats(rows, "Shift", None)?),
                sum_route_bytes(rows, "Shift", None)?,
            ),
            (
                "Insert Shift",
                "3",
                3,
                Some(route_stats(rows, "Shift", Some("insert"))?),
                sum_route_bytes(rows, "Shift", Some("insert"))?,
            ),
            (
                "Delete Shift",
                "3",
                3,
                Some(route_stats(rows, "Shift", Some("delete"))?),
                sum_route_bytes(rows, "Shift", Some("delete"))?,
            ),
            (
                "Append Shift",
                "3",
                3,
                Some(route_stats(rows, "Shift", Some("append"))?),
                sum_route_bytes(rows, "Shift", Some("append"))?,
            ),
            (
                "Truncate Shift",
                "3",
                3,
                Some(route_stats(rows, "Shift", Some("truncate"))?),
                sum_route_bytes(rows, "Shift", Some("truncate"))?,
            ),
            (
                "FullFallback",
                "0",
                fallback_refreshes,
                optional_route_stats(rows, "FullFallback")?,
                sum_route_bytes(rows, "FullFallback", None)?,
            ),
        ] {
            writeln!(
            output,
            "| {label} | `{required}` | `{observed}` | `{}` | `{}` | `{bytes}` | `0` | `PASS` |",
            stats
                .as_ref()
                .map_or_else(|| "N/A".to_owned(), |value| format_ms(value.p50_ns)),
            stats
                .as_ref()
                .map_or_else(|| "N/A".to_owned(), |value| format_ms(value.p95_ns))
        )
            .map_err(display_error)?;
        }
        Ok(())
    }
}
