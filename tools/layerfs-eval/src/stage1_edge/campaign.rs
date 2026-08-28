use super::artifact::{durable_replace, durable_write, io_error, sha256_file, unix_ns};
use super::context::{begin_failure_context, failure_observation, Disposition};
use super::failure_artifacts::write_failure_artifacts;
use super::limits::{CAMPAIGN_LIMIT_NS, INITIAL_BYTES};
use super::receipt_model::{OracleReceipt, Phase, RowReceipt};
use super::resources::{observe_external_resources, unavailable_defaults};
use super::run::run_inner;
use super::schedule::frozen_schedule;
use super::schedule_model::{FrozenSchedule, ScheduledRow};
use crate::stage1_fixture::EvalResult;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::time::Instant;
pub(crate) struct Campaign<'a> {
    pub(crate) run: &'a Path,
    pub(crate) started: Instant,
    pub(crate) started_unix_ns: u128,
    pub(crate) rows: File,
    pub(crate) schedule: &'a FrozenSchedule,
    pub(crate) next_row: usize,
    pub(crate) row_wall_sum_ns: u128,
    pub(crate) fd_baseline: u64,
    pub(crate) rss_peak_bytes: u64,
    pub(crate) q_high_water_bytes: u64,
    pub(crate) q_maximum_terminal_bytes: u64,
    pub(crate) store_connection_high_water: u64,
    pub(crate) physical_oracles: u64,
    pub(crate) canonical_transitions: u64,
    pub(crate) workspace_materializations: u64,
    pub(crate) rematerializations: u64,
    pub(crate) root_digests: Vec<String>,
}
pub(crate) fn enforce_campaign_limit(started: Instant) -> EvalResult<()> {
    if started.elapsed().as_nanos() >= CAMPAIGN_LIMIT_NS {
        Err("complete_wall_ns < 60,000,000,000".to_owned())
    } else {
        Ok(())
    }
}
impl Campaign<'_> {
    pub(crate) fn scheduled(&self, id: &str) -> EvalResult<ScheduledRow> {
        if enforce_campaign_limit(self.started).is_err() {
            begin_failure_context("__between_rows__", "time_budget");
            return Err("complete_wall_ns < 60,000,000,000".to_owned());
        }
        begin_failure_context(id, "admission");
        let row = self
            .schedule
            .rows
            .get(self.next_row)
            .ok_or_else(|| format!("no scheduled row remains for {id}"))?;
        if row.row_id != id {
            return Err(format!(
                "row order mismatch: expected {}, got {id}",
                row.row_id
            ));
        }
        Ok(row.clone())
    }
    pub(crate) fn append(&mut self, receipt: RowReceipt) -> EvalResult<()> {
        enforce_campaign_limit(self.started)?;
        if receipt.schedule.row_index != self.next_row {
            return Err(format!(
                "append row index {} != {}",
                receipt.schedule.row_index, self.next_row
            ));
        }
        let retained_digest = (receipt.status == "PASS"
            && matches!(receipt.schedule.row_group, "C02" | "C03" | "C05" | "C07"))
        .then(|| receipt.oracle.content_digest.clone());
        if retained_digest.as_ref().is_some_and(String::is_empty) {
            return Err("retained root digest is empty".to_owned());
        }
        let json = receipt.json()?;
        self.rows.write_all(json.as_bytes()).map_err(io_error)?;
        self.rows.sync_all().map_err(io_error)?;
        self.row_wall_sum_ns = self
            .row_wall_sum_ns
            .checked_add(receipt.row_wall_ns)
            .ok_or_else(|| "row wall sum overflow".to_owned())?;
        self.rss_peak_bytes = self.rss_peak_bytes.max(receipt.resources.rss_peak_bytes);
        self.store_connection_high_water = self
            .store_connection_high_water
            .max(receipt.resources.active_store_connections);
        if let Some(operation) = receipt.operation {
            self.q_high_water_bytes = self
                .q_high_water_bytes
                .max(operation.operation_q_high_water_bytes);
            self.q_maximum_terminal_bytes = self
                .q_maximum_terminal_bytes
                .max(operation.operation_q_terminal_bytes);
            self.workspace_materializations = self
                .workspace_materializations
                .checked_add(operation.workspace_materializations)
                .ok_or_else(|| "workspace materialization count overflow".to_owned())?;
            self.rematerializations = self
                .rematerializations
                .checked_add(operation.rematerializations)
                .ok_or_else(|| "rematerialization count overflow".to_owned())?;
        }
        self.next_row += 1;
        if let Some(digest) = retained_digest {
            self.root_digests.push(digest);
        }
        Ok(())
    }
}
pub(crate) fn run(run: &Path) -> EvalResult<()> {
    if run.exists() {
        return Err(format!("refusing to overwrite {}", run.display()));
    }
    if cfg!(debug_assertions) {
        return Err("Stage 1.1 campaign requires the release evaluator".to_owned());
    }
    let failure_started = Instant::now();
    let failure_started_unix_ns = unix_ns()?;
    match run_inner(run) {
        Ok(Disposition::Pass) => Ok(()),
        Ok(Disposition::Revise) => Err(format!(
            "Stage 1.1 REVISE artifact preserved at {}",
            run.display()
        )),
        Ok(Disposition::Fail) => Err("Stage 1.1 FAIL disposition".to_owned()),
        Err(error) => {
            if run.exists() {
                let stderr = format!("{error}\n");
                let path = run.join("stderr.txt");
                if path.exists() {
                    let _ = durable_replace(&path, &stderr);
                } else {
                    let _ = durable_write(&path, &stderr);
                }
                let _ = append_failed_row(run, &error, &path);
                let _ = write_failure_artifacts(
                    run,
                    &error,
                    failure_started_unix_ns,
                    failure_started.elapsed().as_nanos(),
                );
            }
            Err(error)
        }
    }
}
pub(crate) fn append_failed_row(run: &Path, error: &str, stderr: &Path) -> EvalResult<()> {
    let schedule = frozen_schedule()?;
    let rows_path = run.join("rows.jsonl");
    let existing = fs::read_to_string(&rows_path).unwrap_or_default();
    let index = existing.lines().count();
    let scheduled = match schedule.rows.get(index) {
        Some(row) => row.clone(),
        None => return Ok(()),
    };
    let (context_row_id, phase, row_wall_ns) = failure_observation();
    if context_row_id == "__between_rows__" {
        return Ok(());
    }
    if context_row_id != scheduled.row_id {
        return Err(format!(
            "failure context row {context_row_id} != next scheduled {}",
            scheduled.row_id
        ));
    }
    let (before_bytes, after_bytes, edit) = if let Some(edit_index) = scheduled.edit_index {
        let edit = schedule.edits[edit_index].clone();
        (edit.before_bytes, edit.after_bytes, Some(edit))
    } else if let Some(burst_index) = scheduled.burst_index {
        let burst = &schedule.bursts[burst_index];
        (
            burst
                .edits
                .first()
                .map_or(INITIAL_BYTES, |edit| edit.before_bytes),
            burst
                .edits
                .last()
                .map_or(INITIAL_BYTES, |edit| edit.after_bytes),
            None,
        )
    } else {
        (INITIAL_BYTES, INITIAL_BYTES, None)
    };
    let work = run.join(".work");
    let resources =
        observe_external_resources(Some(&work), Some(&work.join("store"))).unwrap_or_default();
    let receipt = RowReceipt {
        schedule: scheduled,
        status: "FAIL",
        before_bytes,
        after_bytes,
        edit,
        sub_edits: Vec::new(),
        history_probes: Vec::new(),
        pre_ref: None,
        post_ref: None,
        native_route: "NotApplicable".to_owned(),
        tree_level_before: None,
        phases: vec![Phase {
            name: phase,
            wall_ns: row_wall_ns,
        }],
        phase_counters: Vec::new(),
        row_wall_ns,
        row_residual_ns: 0,
        engine: None,
        operation: None,
        storage_before: None,
        storage_after: None,
        resources,
        oracle: OracleReceipt {
            logical_length: after_bytes,
            ..OracleReceipt::default()
        },
        unavailable: unavailable_defaults(),
        error: Some((
            "EvaluatorOrProductGate".to_owned(),
            error.to_owned(),
            phase.to_owned(),
            error.to_owned(),
            Some(sha256_file(stderr)?),
        )),
        custody: None,
    };
    let mut rows = OpenOptions::new()
        .append(true)
        .open(&rows_path)
        .map_err(io_error)?;
    rows.write_all(receipt.json()?.as_bytes())
        .map_err(io_error)?;
    rows.sync_all().map_err(io_error)
}
pub(crate) fn null_map(keys: &[&str]) -> String {
    format!(
        "{{{}}}",
        keys.iter()
            .map(|key| format!("\"{key}\":null"))
            .collect::<Vec<_>>()
            .join(",")
    )
}
pub(crate) fn optional_artifact_sha256(path: &Path) -> EvalResult<String> {
    if path.is_file() {
        Ok(format!("\"{}\"", sha256_file(path)?))
    } else {
        Ok("null".to_owned())
    }
}
