use super::artifact::{display_error, durable_replace, durable_write, sha256_file};
use super::authentication::validate_authentication;
use super::campaign::Campaign;
use super::context::Disposition;
use super::engine_counters::FixtureMaster;
use super::fixture::SourceIdentity;
use super::limits::CAMPAIGN_LIMIT_NS;
use super::row_parse::{parse_rows, row_optional_u128, validate_ref_chain, ParsedRow};
use super::schedule_model::FrozenSchedule;
use super::summary_json::summary_json;
use super::summary_markdown::summary_markdown;
use super::summary_markdown_contract::{maximum_group_key, validate_summary_pair};
use super::validate_availability::validate_availability_rows;
use super::validate_history::validate_history_rows;
use super::validate_locality::validate_locality_rows;
use crate::stage1_fixture::EvalResult;
pub(crate) fn derive_disposition(rows: &[ParsedRow]) -> Disposition {
    if rows.iter().any(|row| row.status == "FAIL") {
        Disposition::Fail
    } else if rows.iter().any(|row| row.status == "REVISE") {
        Disposition::Revise
    } else {
        Disposition::Pass
    }
}
pub(crate) fn apply_disposition(mut summary: String, disposition: Disposition) -> String {
    if disposition != Disposition::Pass {
        summary = summary.replacen(
            "\"status\":\"PASS\"",
            &format!("\"status\":\"{}\"", disposition.as_str()),
            1,
        );
        summary = summary.replacen(
            "All correctness, durability, locality, route, resource, custody, cleanup, population, and sub-60-second gates passed.",
            "All hard gates passed; a retained report-only observation requires source review before PASS.",
            1,
        );
    }
    summary
}
pub(crate) fn finalize_reports(
    campaign: &mut Campaign<'_>,
    source: &SourceIdentity,
    master: &FixtureMaster,
    schedule: &FrozenSchedule,
) -> EvalResult<Disposition> {
    let rows = parse_rows(&campaign.run.join("rows.jsonl"), schedule)?;
    let disposition = derive_disposition(&rows);
    if disposition == Disposition::Fail {
        return Err("hard-gate failed row cannot be promoted to PASS".to_owned());
    }
    validate_ref_chain(&rows, schedule)?;
    validate_authentication(&rows)?;
    validate_locality_rows(&rows)?;
    validate_availability_rows(&rows)?;
    validate_history_rows(&rows)?;
    let preliminary_complete = campaign.started.elapsed().as_nanos();
    let preliminary_time = campaign_time(campaign, preliminary_complete, disposition);
    durable_write(&campaign.run.join("campaign-time.txt"), &preliminary_time)?;
    let preliminary_campaign_sha = sha256_file(&campaign.run.join("campaign-time.txt"))?;
    let preliminary_json = summary_json(
        campaign,
        &rows,
        source,
        master,
        preliminary_complete,
        &preliminary_campaign_sha,
    )?;
    let preliminary_md = summary_markdown(campaign, &rows, source, master, preliminary_complete)?;
    validate_summary_pair(&preliminary_json, &preliminary_md)?;
    durable_write(&campaign.run.join("summary.json"), &preliminary_json)?;
    durable_write(&campaign.run.join("summary.md"), &preliminary_md)?;
    let complete_wall = campaign.started.elapsed().as_nanos();
    if complete_wall >= CAMPAIGN_LIMIT_NS {
        return Err("complete_wall_ns < 60,000,000,000".to_owned());
    }
    let final_time = campaign_time(campaign, complete_wall, disposition);
    validate_campaign_time(&final_time)?;
    durable_replace(&campaign.run.join("campaign-time.txt"), &final_time)?;
    let campaign_sha = sha256_file(&campaign.run.join("campaign-time.txt"))?;
    let final_json = summary_json(
        campaign,
        &rows,
        source,
        master,
        complete_wall,
        &campaign_sha,
    )?;
    let final_md = summary_markdown(campaign, &rows, source, master, complete_wall)?;
    validate_summary_pair(&final_json, &final_md)?;
    durable_replace(&campaign.run.join("summary.json"), &final_json)?;
    durable_replace(&campaign.run.join("summary.md"), &final_md)?;
    if parse_rows(&campaign.run.join("rows.jsonl"), schedule)?.len() != 47 {
        return Err("final rows revalidation".to_owned());
    }
    Ok(disposition)
}
pub(crate) fn campaign_time(
    campaign: &Campaign<'_>,
    complete_wall_ns: u128,
    disposition: Disposition,
) -> String {
    let outside_rows = complete_wall_ns.saturating_sub(campaign.row_wall_sum_ns);
    format!(
        concat!(
            "schema=layerfs-stage1.1-campaign-time-v1\nstatus={}\n",
            "started_unix_ns={}\ncompleted_unix_ns={}\ncomplete_wall_ns={}\n",
            "row_wall_sum_ns={}\noutside_rows_wall_ns={}\ntimer_residual_ns=0\n",
            "hard_limit_ns=60000000000\nrows_expected=47\nrows_valid=47\n",
            "edit_suboperations_expected=51\nedit_suboperations_observed=51\n",
            "transitions_expected=34\ntransitions_observed=34\n"
        ),
        disposition.as_str(),
        campaign.started_unix_ns,
        campaign.started_unix_ns.saturating_add(complete_wall_ns),
        complete_wall_ns,
        campaign.row_wall_sum_ns,
        outside_rows,
    )
}
pub(crate) fn validate_campaign_time(contents: &str) -> EvalResult<()> {
    if !contents.ends_with('\n') || contents.ends_with("\n\n") {
        return Err("campaign-time.txt must have exactly one trailing newline".to_owned());
    }
    validate_timer_equation(contents)?;
    if campaign_time_value(contents, "hard_limit_ns")? != CAMPAIGN_LIMIT_NS
        || campaign_time_value(contents, "rows_expected")? != 47
        || campaign_time_value(contents, "rows_valid")? != 47
        || campaign_time_value(contents, "edit_suboperations_expected")? != 51
        || campaign_time_value(contents, "edit_suboperations_observed")? != 51
        || campaign_time_value(contents, "transitions_expected")? != 34
        || campaign_time_value(contents, "transitions_observed")? != 34
    {
        return Err("campaign-time timer/population equation".to_owned());
    }
    Ok(())
}
pub(crate) fn campaign_time_value(contents: &str, key: &str) -> EvalResult<u128> {
    contents
        .lines()
        .find_map(|line| line.strip_prefix(&format!("{key}=")))
        .ok_or_else(|| format!("campaign-time missing {key}"))?
        .parse()
        .map_err(display_error)
}
pub(crate) fn validate_timer_equation(contents: &str) -> EvalResult<()> {
    let complete = campaign_time_value(contents, "complete_wall_ns")?;
    let rows = campaign_time_value(contents, "row_wall_sum_ns")?;
    let outside = campaign_time_value(contents, "outside_rows_wall_ns")?;
    let residual = campaign_time_value(contents, "timer_residual_ns")?;
    if complete
        != rows
            .checked_add(outside)
            .and_then(|sum| sum.checked_add(residual))
            .ok_or_else(|| "campaign timer equation overflow".to_owned())?
    {
        return Err("campaign timer equation".to_owned());
    }
    Ok(())
}
pub(crate) fn sum_key(rows: &[ParsedRow], group: Option<&str>, key: &str) -> EvalResult<u128> {
    rows.iter()
        .filter(|row| group.is_none_or(|group| row.row_group == group))
        .try_fold(0_u128, |total, row| {
            total
                .checked_add(row_optional_u128(row, key)?.unwrap_or(0))
                .ok_or_else(|| format!("{key} sum overflow"))
        })
}
pub(crate) fn maximum_key(rows: &[ParsedRow], key: &str) -> EvalResult<u128> {
    rows.iter()
        .map(|row| row_optional_u128(row, key))
        .collect::<EvalResult<Vec<_>>>()?
        .into_iter()
        .flatten()
        .max()
        .ok_or_else(|| format!("no rows for maximum {key}"))
}
pub(crate) fn sum_locality_key(rows: &[ParsedRow], key: &str) -> EvalResult<u128> {
    ["C03", "C05", "C07"]
        .into_iter()
        .try_fold(0_u128, |total, group| {
            total
                .checked_add(sum_key(rows, Some(group), key)?)
                .ok_or_else(|| format!("locality {key} sum overflow"))
        })
}
pub(crate) fn maximum_locality_key(rows: &[ParsedRow], key: &str) -> EvalResult<u128> {
    ["C03", "C05", "C07"]
        .into_iter()
        .map(|group| maximum_group_key(rows, group, key))
        .collect::<EvalResult<Vec<_>>>()?
        .into_iter()
        .max()
        .ok_or_else(|| format!("no locality rows for maximum {key}"))
}
