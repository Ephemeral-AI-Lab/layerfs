use super::artifact::{durable_replace, durable_write, fd_count, io_error, sync_rows};
use super::campaign_execute::execute_campaign;
use super::environment::{environment, environment_json};
use super::model::{Campaign, CampaignData, CAMPAIGN_LIMIT_NS, RESET_COUNT};
use super::readiness::{admit_readiness, schedule_json};
use super::resource_evidence::{append_a16, append_failure_a16, summary_json, terminal_resources};
use super::summary_evidence::summary_markdown;
use crate::stage1_fixture::{
    assert_apfs, fixture_root, read_master, regular_file_ceiling_preflight, verify_master,
    verify_user_file_ceiling, EvalResult,
};
use std::fs::{self, File, OpenOptions};
use std::path::Path;
use std::time::Instant;
pub(crate) fn run_single_file(run: &Path) -> EvalResult<()> {
    if run.exists() {
        return Err(format!(
            "refusing to overwrite run artifact root {}",
            run.display()
        ));
    }
    if cfg!(debug_assertions) {
        return Err("the frozen campaign requires the release evaluator".to_owned());
    }
    fs::create_dir(run).map_err(io_error)?;
    let started = Instant::now();
    durable_write(
        &run.join("stderr.txt"),
        "campaign did not reach its terminal receipt boundary\n",
    )?;
    durable_write(&run.join("summary.json"), &incomplete_summary_json(0))?;
    durable_write(&run.join("campaign-time.txt"), "0\n")?;
    durable_write(&run.join("rows.jsonl"), "")?;
    arm_hard_stop();
    let mut campaign_data = CampaignData::default();
    let mut start_master_receipt = "Unavailable".to_owned();
    let mut final_master_receipt = "Unavailable".to_owned();
    let mut terminal_receipt = None;
    let mut fd_baseline = None;
    let result = (|| {
        let environment = environment()?;
        durable_write(
            &run.join("environment.json"),
            &environment_json(&environment),
        )?;
        durable_write(&run.join("schedule.json"), &schedule_json(false))?;
        regular_file_ceiling_preflight()?;
        verify_user_file_ceiling(&fixture_root().join("input"))?;
        let master = read_master(&fixture_root())?;
        let start_master_digest = verify_master(&fixture_root(), &master, true)?;
        start_master_receipt.clone_from(&start_master_digest);
        let apfs_identity = assert_apfs(&fixture_root())?;
        admit_readiness(&environment, &start_master_digest, &apfs_identity)?;
        fs::copy(fixture_root().join("master.json"), run.join("master.json")).map_err(io_error)?;
        durable_write(&run.join("schedule.json"), &schedule_json(true))?;
        let rows = OpenOptions::new()
            .append(true)
            .open(run.join("rows.jsonl"))
            .map_err(io_error)?;
        let mut campaign = Campaign {
            run,
            started,
            rows,
            data: &mut campaign_data,
        };
        campaign.observe_process_resources("campaign-baseline")?;
        let observed_fd_baseline = fd_count()?;
        fd_baseline = Some(observed_fd_baseline);
        execute_campaign(&mut campaign, &master)?;
        campaign.check_deadline()?;
        if campaign.data.reset_count != RESET_COUNT {
            return Err(format!(
                "reset cardinality mismatch: {} != {RESET_COUNT}",
                campaign.data.reset_count
            ));
        }
        let final_master_digest = verify_master(&fixture_root(), &master, true)?;
        final_master_receipt.clone_from(&final_master_digest);
        if final_master_digest != start_master_digest {
            return Err("sealed master changed during campaign".to_owned());
        }
        let terminal = terminal_resources(observed_fd_baseline)?;
        terminal_receipt = Some(terminal.clone());
        let terminal_error = append_a16(&mut campaign, &terminal, true)?;
        let artifact_started = Instant::now();
        campaign.rows.sync_all().map_err(io_error)?;
        if let Some(error) = terminal_error {
            return Err(error);
        }
        durable_write(
            &run.join("summary.md"),
            "# LayerFS Stage One Part 1\n\nFAIL: campaign did not reach its terminal receipt boundary.\n",
        )?;
        let provisional_wall = campaign.started.elapsed().as_nanos();
        durable_write(
            &run.join("summary.json"),
            &incomplete_summary_json(provisional_wall),
        )?;
        durable_write(
            &run.join("campaign-time.txt"),
            &format!("{provisional_wall}\n"),
        )?;
        campaign.data.artifact_wall_ns = campaign
            .data
            .artifact_wall_ns
            .checked_add(artifact_started.elapsed().as_nanos())
            .ok_or_else(|| "artifact timer overflow".to_owned())?;
        let wall = campaign.started.elapsed().as_nanos();
        let summary = summary_json(
            "PASS",
            None,
            campaign.data,
            wall,
            &start_master_digest,
            &final_master_digest,
            &terminal,
        )?;
        if wall > CAMPAIGN_LIMIT_NS {
            return Err(format!(
                "hard campaign stop exceeded: {wall}ns > {CAMPAIGN_LIMIT_NS}ns"
            ));
        }
        // The provisional durable summary/time writes are inside `wall`. These
        // final receipt rewrites publish that endpoint and are necessarily
        // outside the timestamp they report.
        durable_write(
            &run.join("summary.md"),
            &summary_markdown(campaign.data, wall)?,
        )?;
        durable_write(&run.join("campaign-time.txt"), &format!("{wall}\n"))?;
        fs::remove_file(run.join("stderr.txt")).map_err(io_error)?;
        File::open(run)
            .and_then(|directory| directory.sync_all())
            .map_err(io_error)?;
        durable_replace(&run.join("summary.json"), &summary)?;
        Ok(())
    })();
    if let Err(error) = &result {
        let artifact_started = Instant::now();
        let a16_error = if terminal_receipt.is_none() {
            match append_failure_a16(run, started, &mut campaign_data, fd_baseline) {
                Ok(terminal) => {
                    terminal_receipt = Some(terminal);
                    None
                }
                Err(error) => Some(error),
            }
        } else {
            None
        };
        let rows_sync = sync_rows(run);
        let mut diagnostic = error.clone();
        if let Some(a16) = a16_error {
            diagnostic.push_str(&format!("; A16 append failed: {a16}"));
        }
        if let Err(sync) = &rows_sync {
            diagnostic.push_str(&format!("; rows sync failed: {sync}"));
        }
        let provisional_wall = started.elapsed().as_nanos();
        let _ = durable_write(
            &run.join("campaign-time.txt"),
            &format!("{provisional_wall}\n"),
        );
        let _ = durable_write(&run.join("stderr.txt"), &format!("{diagnostic}\n"));
        let _ = durable_write(
            &run.join("summary.md"),
            &format!("# LayerFS Stage One Part 1\n\nFAIL: {diagnostic}\n"),
        );
        campaign_data.artifact_wall_ns = campaign_data
            .artifact_wall_ns
            .saturating_add(artifact_started.elapsed().as_nanos());
        let wall = started.elapsed().as_nanos();
        let terminal = terminal_receipt.as_ref().cloned().unwrap_or_default();
        if let Ok(summary) = summary_json(
            "FAIL",
            Some(&diagnostic),
            &campaign_data,
            wall,
            &start_master_receipt,
            &final_master_receipt,
            &terminal,
        ) {
            let _ = durable_write(&run.join("campaign-time.txt"), &format!("{wall}\n"));
            let _ = durable_replace(&run.join("summary.json"), &summary);
        }
    }
    cancel_hard_stop();
    result
}
pub(crate) fn incomplete_summary_json(wall: u128) -> String {
    format!(
        "{{\"schema\":\"layerfs-stage1-summary-v2\",\"status\":\"FAIL\",\"campaign_complete_wall_ns\":{wall},\"campaign_equation\":{{\"reset_ns\":0,\"open_ns\":0,\"managed_prepare_ns\":0,\"operation_ns\":0,\"postcheck_ns\":0,\"cleanup_ns\":0,\"artifact_ns\":0,\"timer_residual_ns\":{wall},\"sum_ns\":{wall},\"closed\":true}},\"resets\":0,\"statistics\":{{}},\"targets\":{{}},\"process_resources\":{{\"observations\":[],\"first_64_mib_crossing\":null}},\"error\":\"campaign did not reach its terminal receipt boundary\"}}\n"
    )
}
#[cfg(target_os = "macos")]
pub(crate) fn arm_hard_stop() {
    unsafe extern "C" {
        fn alarm(seconds: u32) -> u32;
    }
    // SAFETY: process-global SIGALRM retains its default terminating action;
    // this campaign owns the evaluator process and has no other alarm user.
    unsafe {
        alarm(120);
    }
}
#[cfg(not(target_os = "macos"))]
pub(crate) fn arm_hard_stop() {}
#[cfg(target_os = "macos")]
pub(crate) fn cancel_hard_stop() {
    unsafe extern "C" {
        fn alarm(seconds: u32) -> u32;
    }
    // SAFETY: cancelling the evaluator-owned alarm has no pointer/state input.
    unsafe {
        alarm(0);
    }
}
#[cfg(not(target_os = "macos"))]
pub(crate) fn cancel_hard_stop() {}
