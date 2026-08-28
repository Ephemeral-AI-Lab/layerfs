use super::artifact::{
    attempt_residue_count, current_rss_bytes, fd_count, io_error, json_escape, maximum_rss_bytes,
    open_store_connection_count,
};
use super::model::{Campaign, CampaignData, ProcessResources};
use super::operation_evidence::{observed_u64_json, option_u64_json};
use super::summary_evidence::{
    performance_disposition, process_resource_summary_json, statistics, statistics_json,
    target_json, validate_metric_populations,
};
use crate::stage1_fixture::{EvalResult, FILE_BYTES};
use std::fs::OpenOptions;
use std::path::Path;
use std::time::Instant;
pub(crate) fn timer_residual(total: u128, attributed: u128) -> EvalResult<u128> {
    total
        .checked_sub(attributed)
        .ok_or_else(|| "attributed wall exceeds enclosing timer".to_owned())
}
#[derive(Clone, Debug, Default)]
pub(crate) struct TerminalResources {
    pub(crate) observed: bool,
    pub(crate) observation_error: Option<String>,
    pub(crate) fd_baseline: u64,
    pub(crate) fd_terminal: u64,
    pub(crate) attempt_residue: u64,
    pub(crate) open_store_connections: u64,
    pub(crate) current_rss_bytes: u64,
    pub(crate) maximum_rss_bytes: u64,
}
pub(crate) struct Statistics {
    pub(crate) sorted: Vec<u128>,
    pub(crate) minimum: u128,
    pub(crate) maximum: u128,
    pub(crate) range: u128,
    pub(crate) p50: u128,
    pub(crate) p95: u128,
    pub(crate) operation_wall: u128,
}
pub(crate) fn terminal_resources(fd_baseline: u64) -> EvalResult<TerminalResources> {
    Ok(TerminalResources {
        observed: true,
        observation_error: None,
        fd_baseline,
        fd_terminal: fd_count()?,
        attempt_residue: attempt_residue_count()?,
        open_store_connections: open_store_connection_count()?,
        current_rss_bytes: current_rss_bytes()?,
        maximum_rss_bytes: maximum_rss_bytes()?,
    })
}
pub(crate) fn append_a16(
    campaign: &mut Campaign<'_>,
    terminal: &TerminalResources,
    master_unchanged: bool,
) -> EvalResult<Option<String>> {
    let failed = !terminal.observed
        || terminal.fd_terminal != terminal.fd_baseline
        || terminal.attempt_residue != 0
        || terminal.open_store_connections != 0
        || terminal.maximum_rss_bytes > 67_108_864
        || campaign.data.last_q_terminal_bytes != Some(0)
        || !master_unchanged;
    let error = failed.then(|| match terminal.observation_error.as_deref() {
        Some(observation) => format!(
            "A16 terminal resource observation unavailable: {observation}; Q {:?}, master unchanged {}",
            campaign.data.last_q_terminal_bytes, master_unchanged,
        ),
        None => format!(
            "A16 terminal resource equation failed: fd {}/{}, residue {}, connections {}, current RSS {}, peak RSS {}, Q {:?}, master unchanged {}",
            terminal.fd_terminal,
            terminal.fd_baseline,
            terminal.attempt_residue,
            terminal.open_store_connections,
            terminal.current_rss_bytes,
            terminal.maximum_rss_bytes,
            campaign.data.last_q_terminal_bytes,
            master_unchanged,
        ),
    });
    let resources = ProcessResources {
        operation: "A16".to_owned(),
        observed: terminal.observed,
        current_rss_bytes: terminal.current_rss_bytes,
        process_peak_rss_bytes: terminal.maximum_rss_bytes,
    };
    campaign.row_with_resources(
        format!(
            "{{\"id\":\"A16\",\"gate_status\":\"{}\",\"gate_error\":{},\"terminal\":{{\"observed\":{},\"observation_error\":{},\"operation_q_bytes\":{},\"fd_baseline\":{},\"fd_terminal\":{},\"active_store_connections\":{},\"owned_temp_journal_attempt_residue\":{},\"current_rss_bytes\":{},\"process_peak_rss_bytes\":{},\"maximum_rss_bytes\":{}}},\"store_database_bytes_max\":{},\"maximum_user_regular_file_bytes\":{FILE_BYTES},\"master_unchanged\":{master_unchanged}}}",
            if failed { "FAIL" } else { "PASS" },
            error
                .as_deref()
                .map(|value| format!("\"{}\"", json_escape(value)))
                .unwrap_or_else(|| "null".to_owned()),
            terminal.observed,
            terminal
                .observation_error
                .as_deref()
                .map(|value| format!("\"{}\"", json_escape(value)))
                .unwrap_or_else(|| "null".to_owned()),
            option_u64_json(campaign.data.last_q_terminal_bytes),
            observed_u64_json(terminal.observed, terminal.fd_baseline),
            observed_u64_json(terminal.observed, terminal.fd_terminal),
            observed_u64_json(terminal.observed, terminal.open_store_connections),
            observed_u64_json(terminal.observed, terminal.attempt_residue),
            observed_u64_json(terminal.observed, terminal.current_rss_bytes),
            observed_u64_json(terminal.observed, terminal.maximum_rss_bytes),
            observed_u64_json(terminal.observed, terminal.maximum_rss_bytes),
            option_u64_json(campaign.data.store_database_bytes_max),
        ),
        resources,
    )?;
    Ok(error)
}
pub(crate) fn append_failure_a16(
    run: &Path,
    started: Instant,
    data: &mut CampaignData,
    fd_baseline: Option<u64>,
) -> EvalResult<TerminalResources> {
    let terminal = match fd_baseline {
        Some(fd_baseline) => {
            terminal_resources(fd_baseline).unwrap_or_else(|error| TerminalResources {
                observation_error: Some(error),
                ..TerminalResources::default()
            })
        }
        None => TerminalResources {
            observation_error: Some("FD baseline unavailable".to_owned()),
            ..TerminalResources::default()
        },
    };
    let rows = OpenOptions::new()
        .append(true)
        .open(run.join("rows.jsonl"))
        .map_err(io_error)?;
    let mut campaign = Campaign {
        run,
        started,
        rows,
        data,
    };
    let _ = append_a16(&mut campaign, &terminal, false)?;
    campaign.rows.sync_all().map_err(io_error)?;
    Ok(terminal)
}
pub(crate) fn process_resources(operation: &str) -> EvalResult<ProcessResources> {
    Ok(ProcessResources {
        operation: operation.to_owned(),
        observed: true,
        current_rss_bytes: current_rss_bytes()?,
        process_peak_rss_bytes: maximum_rss_bytes()?,
    })
}
pub(crate) fn process_resources_json(value: &ProcessResources) -> String {
    let crossed = if value.observed {
        (value.process_peak_rss_bytes > 67_108_864).to_string()
    } else {
        "\"Unavailable\"".to_owned()
    };
    format!(
        "{{\"operation\":\"{}\",\"observed\":{},\"current_rss_bytes\":{},\"process_peak_rss_bytes\":{},\"crossed_64_mib\":{crossed}}}",
        json_escape(&value.operation),
        value.observed,
        observed_u64_json(value.observed, value.current_rss_bytes),
        observed_u64_json(value.observed, value.process_peak_rss_bytes),
    )
}
pub(crate) fn summary_json(
    status: &str,
    error: Option<&str>,
    data: &CampaignData,
    wall: u128,
    start_master: &str,
    final_master: &str,
    terminal: &TerminalResources,
) -> EvalResult<String> {
    if status == "PASS" {
        validate_metric_populations(data)?;
    }
    let disposition = if status == "PASS" {
        performance_disposition(data, wall)?
    } else {
        status.to_owned()
    };
    let metrics = data
        .metrics
        .iter()
        .map(|(name, observations)| {
            let statistics = statistics(observations)?;
            Ok(format!(
                "\"{}\":{}",
                json_escape(name),
                statistics_json(
                    name,
                    &statistics,
                    data.bytes_per_observation.get(name).copied()
                )
            ))
        })
        .collect::<EvalResult<Vec<_>>>()?
        .join(",");
    let targets = target_json(data, wall)?;
    let process_resources = process_resource_summary_json(data);
    let roots = data
        .output_roots
        .iter()
        .map(|(name, root)| format!("\"{}\":\"{}\"", json_escape(name), root))
        .collect::<Vec<_>>()
        .join(",");
    let phase_sum = data
        .reset_wall_ns
        .checked_add(data.open_wall_ns)
        .and_then(|value| value.checked_add(data.managed_prepare_wall_ns))
        .and_then(|value| value.checked_add(data.operation_wall_ns))
        .and_then(|value| value.checked_add(data.postcheck_wall_ns))
        .and_then(|value| value.checked_add(data.cleanup_wall_ns))
        .and_then(|value| value.checked_add(data.artifact_wall_ns))
        .ok_or_else(|| "campaign phase sum overflow".to_owned())?;
    if status == "PASS" && phase_sum > wall {
        return Err("campaign phase attribution exceeds complete wall".to_owned());
    }
    let timer_residual = wall.saturating_sub(phase_sum);
    let equation_sum = phase_sum.saturating_add(timer_residual);
    Ok(format!(
        concat!(
            "{{\"schema\":\"layerfs-stage1-summary-v2\",\"status\":\"{}\",\"error\":{},",
            "\"campaign_complete_wall_ns\":{},\"campaign_equation\":{{",
            "\"reset_ns\":{},\"open_ns\":{},\"managed_prepare_ns\":{},",
            "\"operation_ns\":{},\"postcheck_ns\":{},\"cleanup_ns\":{},",
            "\"artifact_ns\":{},\"timer_residual_ns\":{},\"sum_ns\":{},",
            "\"closed\":{}}},",
            "\"resets\":{},\"statistics\":{{{}}},\"targets\":{},",
            "\"process_resources\":{},",
            "\"output_roots\":{{{}}},\"master_start_blake3\":\"{}\",",
            "\"master_final_blake3\":\"{}\",",
            "\"terminal_receipt_rewrites_outside_accounted_wall\":[\"summary.json\",\"summary.md\",\"campaign-time.txt\"],",
            "\"maximum_user_regular_file_bytes\":{},\"store_database_bytes_max\":{},",
            "\"terminal\":{{\"fd_baseline\":{},\"fd_terminal\":{},",
            "\"attempt_residue\":{},\"active_store_connections\":{},",
            "\"current_rss_bytes\":{},\"process_peak_rss_bytes\":{},",
            "\"maximum_rss_bytes\":{},",
            "\"operation_q_bytes\":{}}}}}\n"
        ),
        json_escape(&disposition),
        error
            .map(|value| format!("\"{}\"", json_escape(value)))
            .unwrap_or_else(|| "null".to_owned()),
        wall,
        data.reset_wall_ns,
        data.open_wall_ns,
        data.managed_prepare_wall_ns,
        data.operation_wall_ns,
        data.postcheck_wall_ns,
        data.cleanup_wall_ns,
        data.artifact_wall_ns,
        timer_residual,
        equation_sum,
        equation_sum == wall,
        data.reset_count,
        metrics,
        targets,
        process_resources,
        roots,
        start_master,
        final_master,
        FILE_BYTES,
        option_u64_json(data.store_database_bytes_max),
        observed_u64_json(terminal.observed, terminal.fd_baseline),
        observed_u64_json(terminal.observed, terminal.fd_terminal),
        observed_u64_json(terminal.observed, terminal.attempt_residue),
        observed_u64_json(terminal.observed, terminal.open_store_connections),
        observed_u64_json(terminal.observed, terminal.current_rss_bytes),
        observed_u64_json(terminal.observed, terminal.maximum_rss_bytes),
        observed_u64_json(terminal.observed, terminal.maximum_rss_bytes),
        option_u64_json(data.last_q_terminal_bytes),
    ))
}
