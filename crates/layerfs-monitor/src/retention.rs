use crate::{MonitorResult, OperationReceipt};
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

const MAX_SEGMENT_BYTES: u64 = 32 * 1024 * 1024;
const MAX_SEGMENT_RECEIPTS: u64 = 5_000;
const MAX_AGE: std::time::Duration = std::time::Duration::from_secs(30 * 24 * 60 * 60);

pub(crate) struct Retention {
    path: PathBuf,
    state: Mutex<RetentionState>,
}

struct RetentionState {
    bytes: u64,
    receipts: u64,
}

impl Retention {
    pub(crate) fn new(root: &Path) -> MonitorResult<Self> {
        std::fs::create_dir_all(root)?;
        let path = root.join("operations.jsonl");
        let rotated = path.with_extension("jsonl.1");
        remove_expired(&path)?;
        remove_expired(&rotated)?;
        let bytes = std::fs::metadata(&path)
            .map(|metadata| metadata.len())
            .unwrap_or(0);
        let receipts = std::fs::File::open(&path)
            .map(|file| std::io::BufReader::new(file).lines().count() as u64)
            .unwrap_or(0);
        Ok(Self {
            path,
            state: Mutex::new(RetentionState { bytes, receipts }),
        })
    }

    pub(crate) fn append(&self, receipt: &OperationReceipt) -> MonitorResult<()> {
        let line = receipt.to_json();
        let line_bytes = line.len() as u64 + 1;
        let mut state = self
            .state
            .lock()
            .map_err(|_| crate::MonitorError::Integrity("retention lock"))?;
        if state.receipts >= MAX_SEGMENT_RECEIPTS
            || state.bytes.saturating_add(line_bytes) > MAX_SEGMENT_BYTES
        {
            let rotated = self.path.with_extension("jsonl.1");
            if rotated.exists() {
                std::fs::remove_file(&rotated)?;
            }
            if self.path.exists() {
                std::fs::rename(&self.path, rotated)?;
            }
            state.bytes = 0;
            state.receipts = 0;
        }
        let mut output = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        writeln!(output, "{line}")?;
        output.flush()?;
        state.bytes = state.bytes.saturating_add(line_bytes);
        state.receipts += 1;
        Ok(())
    }

    pub(crate) fn load(&self) -> MonitorResult<Vec<OperationReceipt>> {
        let mut receipts = Vec::new();
        for path in [self.path.with_extension("jsonl.1"), self.path.clone()] {
            let file = match std::fs::File::open(path) {
                Ok(file) => file,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(error) => return Err(error.into()),
            };
            for line in std::io::BufReader::new(file).lines() {
                let line = line?;
                if line.contains("\"record\":\"") {
                    receipts.push(OperationReceipt::from_json(&line)?);
                }
            }
        }
        Ok(receipts)
    }
}

fn remove_expired(path: &Path) -> MonitorResult<()> {
    let expired = std::fs::metadata(path)
        .and_then(|metadata| metadata.modified())
        .ok()
        .and_then(|modified| modified.elapsed().ok())
        .is_some_and(|age| age > MAX_AGE);
    if expired {
        std::fs::remove_file(path)?;
    }
    Ok(())
}
