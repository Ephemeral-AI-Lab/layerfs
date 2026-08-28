use super::campaign::Campaign;
use super::engine_counters::FixtureMaster;
use super::fixture::SourceIdentity;
use super::markdown_report::MarkdownReport;
use super::row_parse::ParsedRow;
use crate::stage1_fixture::EvalResult;
pub(crate) fn summary_markdown(
    campaign: &Campaign<'_>,
    rows: &[ParsedRow],
    source: &SourceIdentity,
    master: &FixtureMaster,
    complete_wall_ns: u128,
) -> EvalResult<String> {
    let mut report = MarkdownReport::new(campaign, rows, source, master, complete_wall_ns)?;
    report.append_custody()?;
    report.append_edits()?;
    report.append_evidence()?;
    report.append_closure()?;
    report.append_disposition()?;
    report.finish()
}
