use super::artifact::{io_error, json_string, json_u128};
use super::campaign_edit::run_edit_matrix;
use super::campaign_history::{run_a15, run_a17};
use super::campaign_read::{run_a01, run_a02};
use super::campaign_refresh::{run_a09, run_a10_a12};
use super::campaign_reset::{run_a13, run_a14};
use super::campaign_write::run_stream_write;
use super::model::{Campaign, ProcessResources, CAMPAIGN_LIMIT_NS, RESET_LIMIT_NS};
use super::resource_evidence::{process_resources, process_resources_json};
use crate::legacy_full::{IntegrityMode, OpenedLayerFs, RootId};
use crate::stage1_fixture::{Attempt, BaseManifest, EvalResult, Master};
use std::io::Write;
use std::time::Instant;
pub(crate) fn execute_campaign(campaign: &mut Campaign<'_>, master: &Master) -> EvalResult<()> {
    run_a01(campaign, master)?;
    run_a02(campaign, master)?;
    run_stream_write(campaign, master, "A03a", "import-genesis", false)?;
    run_stream_write(campaign, master, "A03b", "replace-existing", true)?;
    run_edit_matrix(campaign, master)?;
    run_a09(campaign, master)?;
    run_a10_a12(campaign, master)?;
    run_a13(campaign, master)?;
    run_a14(campaign, master)?;
    run_a15(campaign, master)?;
    run_a17(campaign, master)?;
    Ok(())
}
impl Campaign<'_> {
    pub(crate) fn check_deadline(&self) -> EvalResult<()> {
        if self.started.elapsed().as_nanos() >= CAMPAIGN_LIMIT_NS {
            Err("hard campaign deadline reached".to_owned())
        } else {
            Ok(())
        }
    }
    pub(crate) fn attempt(&mut self, name: &str, expected: &BaseManifest) -> EvalResult<Attempt> {
        self.check_deadline()?;
        let attempt = Attempt::create(name, expected)?;
        if attempt.clone.wall_ns > RESET_LIMIT_NS {
            return Err(format!(
                "measured reset {}ns exceeds {RESET_LIMIT_NS}ns",
                attempt.clone.wall_ns
            ));
        }
        self.data.reset_count = self
            .data
            .reset_count
            .checked_add(1)
            .ok_or_else(|| "reset counter overflow".to_owned())?;
        self.data.reset_wall_ns = self
            .data
            .reset_wall_ns
            .checked_add(attempt.clone.wall_ns)
            .ok_or_else(|| "reset timer overflow".to_owned())?;
        Ok(attempt)
    }
    pub(crate) fn open(
        &mut self,
        attempt: &Attempt,
        expected: &BaseManifest,
    ) -> EvalResult<(OpenedLayerFs, u128)> {
        self.check_deadline()?;
        let started = Instant::now();
        let opened = attempt.open(expected, IntegrityMode::TrustedLocalDev)?;
        let wall = started.elapsed().as_nanos();
        self.data.open_wall_ns = self
            .data
            .open_wall_ns
            .checked_add(wall)
            .ok_or_else(|| "open timer overflow".to_owned())?;
        Ok((opened, wall))
    }
    pub(crate) fn cleanup(&mut self, attempt: Attempt) -> EvalResult<u128> {
        let started = Instant::now();
        attempt.cleanup()?;
        let wall = started.elapsed().as_nanos();
        self.data.cleanup_wall_ns = self
            .data
            .cleanup_wall_ns
            .checked_add(wall)
            .ok_or_else(|| "cleanup timer overflow".to_owned())?;
        Ok(wall)
    }
    pub(crate) fn operation_wall(&mut self, wall: u128) -> EvalResult<()> {
        self.data.operation_wall_ns = self
            .data
            .operation_wall_ns
            .checked_add(wall)
            .ok_or_else(|| "operation timer overflow".to_owned())?;
        Ok(())
    }
    pub(crate) fn postcheck_wall(&mut self, wall: u128) -> EvalResult<()> {
        self.data.postcheck_wall_ns = self
            .data
            .postcheck_wall_ns
            .checked_add(wall)
            .ok_or_else(|| "postcheck timer overflow".to_owned())?;
        Ok(())
    }
    pub(crate) fn metric(&mut self, name: &str, wall: u128, bytes: Option<u64>) -> EvalResult<()> {
        self.data
            .metrics
            .entry(name.to_owned())
            .or_default()
            .push(wall);
        if let Some(bytes) = bytes {
            match self
                .data
                .bytes_per_observation
                .insert(name.to_owned(), bytes)
            {
                Some(previous) if previous != bytes => {
                    return Err(format!("metric {name} byte population changed"));
                }
                _ => {}
            }
        }
        Ok(())
    }
    pub(crate) fn bind_output_root(&mut self, name: &str, root: RootId) -> EvalResult<()> {
        let root = root.to_string();
        match self.data.output_roots.insert(name.to_owned(), root.clone()) {
            Some(previous) if previous != root => {
                Err(format!("{name} output root changed across cloned bases"))
            }
            _ => Ok(()),
        }
    }
    pub(crate) fn observe_process_resources(
        &mut self,
        operation: &str,
    ) -> EvalResult<ProcessResources> {
        let resources = process_resources(operation)?;
        self.data.process_resources.push(resources.clone());
        Ok(resources)
    }
    pub(crate) fn row(&mut self, row: String) -> EvalResult<()> {
        let mut operation = json_string(&row, "id")?;
        if let Ok(arm) = json_string(&row, "arm") {
            operation.push('/');
            operation.push_str(&arm);
        }
        if let Ok(sample) = json_u128(&row, "sample") {
            operation.push_str(&format!("/sample-{sample}"));
        }
        if let Ok(position) = json_string(&row, "position") {
            operation.push('/');
            operation.push_str(&position);
        }
        let resources = process_resources(&operation)?;
        self.row_with_resources(row, resources)
    }
    pub(crate) fn row_with_resources(
        &mut self,
        row: String,
        resources: ProcessResources,
    ) -> EvalResult<()> {
        let started = Instant::now();
        if !self.run.is_dir() {
            return Err("campaign artifact root disappeared".to_owned());
        }
        let row = row
            .strip_suffix('}')
            .map(|body| {
                format!(
                    "{body},\"process_resources\":{}}}",
                    process_resources_json(&resources)
                )
            })
            .ok_or_else(|| "row JSON must be an object".to_owned())?;
        let row = row
            .strip_prefix('{')
            .map(|body| format!("{{\"schema\":\"layerfs-stage1-row-v1\",{body}"))
            .ok_or_else(|| "row JSON must be an object".to_owned())?;
        self.rows.write_all(row.as_bytes()).map_err(io_error)?;
        self.rows.write_all(b"\n").map_err(io_error)?;
        self.rows.flush().map_err(io_error)?;
        self.data.artifact_wall_ns = self
            .data
            .artifact_wall_ns
            .checked_add(started.elapsed().as_nanos())
            .ok_or_else(|| "artifact timer overflow".to_owned())?;
        self.data.process_resources.push(resources);
        Ok(())
    }
    pub(crate) fn store_database(&mut self, bytes: Option<u64>) {
        if let Some(bytes) = bytes {
            self.data.store_database_bytes_max = Some(
                self.data
                    .store_database_bytes_max
                    .map_or(bytes, |current| current.max(bytes)),
            );
        }
    }
}
