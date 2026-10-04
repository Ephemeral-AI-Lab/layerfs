//! History-only cold-boundary observer; all invocation cost stays in lifecycle.
use super::workload::json;
use std::{path::PathBuf, process::Command, time::Instant};

pub struct Boundary {
    helper: Option<PathBuf>,
    files: Vec<PathBuf>,
    pub checks: usize,
    pub wall_ns: u128,
}
impl Boundary {
    pub fn new(files: Vec<PathBuf>, required: bool) -> Result<Self, Box<dyn std::error::Error>> {
        let helper = std::env::var_os("LAYERFS_HISTORY_COLD_HELPER").map(PathBuf::from);
        if required && helper.is_none() {
            return Err("complete history requires sealed native cold helper".into());
        }
        Ok(Self {
            helper,
            files,
            checks: 0,
            wall_ns: 0,
        })
    }
    pub fn check(&mut self, state: usize) -> Result<(), Box<dyn std::error::Error>> {
        let Some(helper) = &self.helper else {
            return Ok(());
        };
        let start = Instant::now();
        let mut files = Vec::new();
        for path in &self.files {
            if !path.is_file() {
                return Err("history cold database missing".into());
            }
            files.push(path.clone());
            let wal = PathBuf::from(format!("{}-wal", path.display()));
            if wal.exists() {
                files.push(wal);
            }
        }
        let output = Command::new(helper).arg("--files").args(&files).output()?;
        let wall_ns = start.elapsed().as_nanos();
        if !output.status.success() {
            return Err(format!(
                "history cold helper failed: {}",
                String::from_utf8_lossy(&output.stderr)
            )
            .into());
        }
        let text = std::str::from_utf8(&output.stdout)?.trim();
        let value = json::parse(text)?;
        if value.get("resident_after").and_then(json::Value::as_i64) != Some(0)
            || value.get("files").and_then(json::Value::as_i64) != Some(files.len() as i64)
        {
            return Err("history state boundary INELIGIBLE: resident database pages".into());
        }
        self.checks += 1;
        self.wall_ns += wall_ns;
        eprintln!("HISTORY_COLD_BOUNDARY {{\"before_state\":{state},\"wall_ns\":{wall_ns},\"attestation\":{text}}}");
        Ok(())
    }
}
