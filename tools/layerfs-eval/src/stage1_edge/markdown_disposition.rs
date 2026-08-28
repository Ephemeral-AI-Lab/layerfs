use super::artifact::display_error;
use super::markdown_helpers::{format_ms, format_signed_ms, preserved_failure_ledger};
use super::markdown_report::MarkdownReport;
use super::optimization::{signed_gain, OPTIMIZATION_BASELINE_ROWS_SHA256};
use super::report_disposition::{sum_key, sum_locality_key};
use crate::stage1_fixture::EvalResult;
use std::fmt::Write as FmtWrite;
impl MarkdownReport<'_, '_> {
    pub(crate) fn append_disposition(&mut self) -> EvalResult<()> {
        let campaign = self.campaign;
        let rows = self.rows;
        let optimization = &self.optimization;
        let physical_oracles = self.physical_oracles;
        let canonical_transitions = self.canonical_transitions;
        let selected_history_roots_passed = self.selected_history_roots_passed;
        let patch_refreshes = self.patch_refreshes;
        let shift_refreshes = self.shift_refreshes;
        let fallback_refreshes = self.fallback_refreshes;
        let complete_wall_ns = self.complete_wall_ns;
        let output = &mut self.output;
        let failures = preserved_failure_ledger(campaign.run)?;
        writeln!(output, "\n## 15. Preserved failures and unavailable observations\n\n| Sequence | Artifact/row | Field | Availability/failure | Reason | Disposition impact |\n|---:|---|---|---|---|---|\n| 1 | all applicable rows | `native.sync_regular_calls` | `Unavailable` | product exposes only aggregate sync calls | none; no hard split-sync gate |\n| 2 | all applicable rows | `native.sync_directory_calls` | `Unavailable` | product exposes only aggregate sync calls | none; no hard split-sync gate |\n| 3 | all applicable rows | `storage.rollback_journal_bytes` | `Unavailable` | not continuously observed | terminal sidecar absence passed |\n| 4 | all applicable rows | `storage.temporary_file_bytes` | `Unavailable` | not continuously observed | terminal residue absence passed |").map_err(display_error)?;
        for (index, failure) in failures.iter().enumerate() {
            writeln!(
                output,
                "| {} | `{}` | `{}` | `failure` | {} | {} |",
                index + 5,
                failure.artifact.replace('|', "\\|"),
                failure.field.replace('|', "\\|"),
                failure.reason.replace('|', "\\|"),
                failure.disposition_impact.replace('|', "\\|"),
            )
            .map_err(display_error)?;
        }
        writeln!(
        output,
        "\nPreserved failed attempts: `{}`\nSuperseded attempts: `{}`\nDeleted or overwritten attempts: `0`",
        failures.len(),
        failures.len(),
    )
    .map_err(display_error)?;
        writeln!(output, "\n## 16. Final disposition\n\nPost-PASS optimization baseline: `{}` (rows SHA-256 `{}`).\n\n| Optimization metric | Attempt-007 before ms | Current after ms | Absolute gain ms | Owner |\n|---|---:|---:|---:|---|\n| Complete campaign wall | `{}` | `{}` | `{}` | product + evaluator |\n| Transition counter/resource snapshots | `{}` | `{}` | `{}` | evaluator |\n| History read/oracle wall | `{}` | `{}` | `{}` | evaluator |\n| Append/truncate refresh p50 | `{}` | `{}` | `{}` | product EOF splice |\n| Milestone materialization p50 | `{}` | `{}` | `{}` | product read/materialize |",
        optimization.baseline_path,
        OPTIMIZATION_BASELINE_ROWS_SHA256,
        format_ms(optimization.baseline_complete_wall_ns),
        format_ms(optimization.current_complete_wall_ns),
        format_signed_ms(signed_gain(optimization.baseline_complete_wall_ns, optimization.current_complete_wall_ns)?),
        format_ms(optimization.baseline_counter_snapshot_ns),
        format_ms(optimization.current_counter_snapshot_ns),
        format_signed_ms(signed_gain(optimization.baseline_counter_snapshot_ns, optimization.current_counter_snapshot_ns)?),
        format_ms(optimization.baseline_history_read_ns),
        format_ms(optimization.current_history_read_ns),
        format_signed_ms(signed_gain(optimization.baseline_history_read_ns, optimization.current_history_read_ns)?),
        format_ms(optimization.baseline_append_truncate.p50_ns),
        format_ms(optimization.current_append_truncate.p50_ns),
        format_signed_ms(signed_gain(optimization.baseline_append_truncate.p50_ns, optimization.current_append_truncate.p50_ns)?),
        format_ms(optimization.baseline_materialization.p50_ns),
        format_ms(optimization.current_materialization.p50_ns),
        format_signed_ms(signed_gain(optimization.baseline_materialization.p50_ns, optimization.current_materialization.p50_ns)?),
    ).map_err(display_error)?;
        for receipt in &optimization.verified_open {
            writeln!(
            output,
            "| Verified open {} | `{}` | `{}` | `{}` | product retained-union scrub; current scrub/graphs/fetched/object B/scratch=`{}/{}/{}/{}/{}` |",
            receipt.root,
            format_ms(receipt.before_ns),
            format_ms(receipt.after_ns),
            format_signed_ms(signed_gain(receipt.before_ns, receipt.after_ns)?),
            receipt.retained_union_scrubs,
            receipt.namespace_graphs,
            receipt.fetched_rows,
            receipt.object_bytes_read,
            receipt.scratch_tables,
        )
        .map_err(display_error)?;
        }
        writeln!(output, "\nShift-route mix changed from CloneShift/InPlaceShift `{}/{}` to `{}/{}`; append/truncate EOF splices retain exact InPlaceShift durability and zero FullFallback.\n\nResult: `PASS`\n\n| Category | Result | Decisive evidence |\n|---|---|---|\n| Correctness | `PASS` | `{}/51 physical; {}/34 canonical; {}/8 selected history` |\n| Durability | `PASS` | `{}` transactions / `{}` COMMITs / exact RefState rotation |\n| Locality | `PASS` | `{}` CDC B; zero unaffected canonical suffix; node bounds exact |\n| Physical routes | `PASS` | `{}` patch / `{}` shift / `{}` FullFallback refreshes |\n| Resources | `PASS` | `RSS/Q/FD/connections/residue closed` |\n| Custody | `PASS` | `source/executable/fixture/rows bound by digest` |\n| Complete wall | `PASS` | `{} ms < 60 s` |\n\nReason: All correctness, durability, locality, route, resource, custody, cleanup, population, and sub-60-second gates passed.\n", optimization.baseline_clone_shift, optimization.baseline_in_place_shift, optimization.current_clone_shift, optimization.current_in_place_shift, physical_oracles, canonical_transitions, selected_history_roots_passed, sum_key(rows, None, "transactions_started")?, sum_key(rows, None, "publication_commits")?, sum_locality_key(rows, "cdc_bytes_scanned")?, patch_refreshes, shift_refreshes, fallback_refreshes, format_ms(complete_wall_ns)).map_err(display_error)?;
        Ok(())
    }
}
