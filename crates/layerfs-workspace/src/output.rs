use crate::{ExecutionReceipt, OutputChunk, OutputStream, WorkspaceError, WorkspaceResult};
use std::collections::VecDeque;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex};

const OUTPUT_TAIL_BYTES: usize = 1024 * 1024;

pub struct OutputReader {
    log: Arc<OutputLog>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OutputPage {
    pub chunks: Vec<OutputChunk>,
    pub next_sequence: u64,
    pub truncated: bool,
    pub exited: bool,
    pub receipt: Option<ExecutionReceipt>,
}

impl OutputReader {
    pub(crate) fn new(log: Arc<OutputLog>) -> Self {
        Self { log }
    }

    pub fn read(&self, after: u64, follow: bool) -> WorkspaceResult<OutputPage> {
        self.log.read(after, follow)
    }
}

pub(crate) struct OutputLog {
    path: PathBuf,
    state: Mutex<OutputState>,
    changed: Condvar,
}

struct OutputState {
    chunks: VecDeque<OutputChunk>,
    bytes: usize,
    next_sequence: u64,
    truncated_through: Option<u64>,
    receipt: Option<ExecutionReceipt>,
}

impl OutputLog {
    pub(crate) fn create(path: &Path) -> WorkspaceResult<Arc<Self>> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        OpenOptions::new().create_new(true).write(true).open(path)?;
        Ok(Arc::new(Self {
            path: path.to_owned(),
            state: Mutex::new(OutputState {
                chunks: VecDeque::new(),
                bytes: 0,
                next_sequence: 0,
                truncated_through: None,
                receipt: None,
            }),
            changed: Condvar::new(),
        }))
    }

    pub(crate) fn append(&self, stream: OutputStream, bytes: &[u8]) -> WorkspaceResult<()> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| WorkspaceError::WorkspaceBusy)?;
        let truncated = bytes.len() > OUTPUT_TAIL_BYTES;
        let bytes = &bytes[bytes.len().saturating_sub(OUTPUT_TAIL_BYTES)..];
        let chunk = OutputChunk {
            sequence: state.next_sequence,
            stream,
            bytes: bytes.to_vec(),
        };
        let sequence = chunk.sequence;
        state.next_sequence += 1;
        state.bytes += chunk.bytes.len();
        append_frame(&self.path, &chunk)?;
        state.chunks.push_back(chunk);
        if truncated {
            state.truncated_through = Some(sequence);
        }
        let mut rewrite = false;
        while state.bytes > OUTPUT_TAIL_BYTES {
            let Some(removed) = state.chunks.pop_front() else {
                break;
            };
            state.bytes -= removed.bytes.len();
            state.truncated_through = Some(
                state
                    .truncated_through
                    .map_or(removed.sequence, |sequence| sequence.max(removed.sequence)),
            );
            rewrite = true;
        }
        if rewrite {
            rewrite_frames(&self.path, &state.chunks)?;
        }
        self.changed.notify_all();
        Ok(())
    }

    pub(crate) fn finish_timed(
        &self,
        mut receipt: ExecutionReceipt,
        execution_receipt: &Mutex<Option<ExecutionReceipt>>,
        total_started: std::time::Instant,
    ) -> Option<ExecutionReceipt> {
        let terminal_started = std::time::Instant::now();
        let mut state = self.state.lock().ok()?;
        let mut stored = execution_receipt.lock().ok()?;
        self.changed.notify_all();
        state.receipt = Some(receipt.clone());
        *stored = Some(receipt.clone());
        receipt.terminal_publication_ns = terminal_started
            .elapsed()
            .as_nanos()
            .min(u128::from(u64::MAX)) as u64;
        receipt.total_wall_ns = total_started.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64;
        receipt.elapsed_ns = receipt.total_wall_ns;
        let attributed = receipt
            .spawn_ns
            .saturating_add(receipt.supervisor_queue_ns)
            .saturating_add(receipt.runtime_ns)
            .saturating_add(receipt.drain_ns)
            .saturating_add(receipt.terminal_publication_ns);
        receipt.unattributed_ns = receipt.total_wall_ns.saturating_sub(attributed);
        state.receipt = Some(receipt.clone());
        *stored = Some(receipt.clone());
        Some(receipt)
    }

    pub(crate) fn retained_bytes(&self) -> u64 {
        self.state
            .lock()
            .map(|state| {
                state
                    .bytes
                    .saturating_add(state.chunks.len().saturating_mul(13)) as u64
            })
            .unwrap_or(0)
    }

    fn read(&self, after: u64, follow: bool) -> WorkspaceResult<OutputPage> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| WorkspaceError::WorkspaceBusy)?;
        if follow
            && state.receipt.is_none()
            && state
                .chunks
                .back()
                .is_none_or(|chunk| chunk.sequence < after)
        {
            let (next, _) = self
                .changed
                .wait_timeout(state, std::time::Duration::from_secs(1))
                .map_err(|_| WorkspaceError::WorkspaceBusy)?;
            state = next;
        }
        let chunks = state
            .chunks
            .iter()
            .filter(|chunk| chunk.sequence >= after)
            .cloned()
            .collect();
        Ok(OutputPage {
            chunks,
            next_sequence: state.next_sequence,
            truncated: state
                .truncated_through
                .is_some_and(|sequence| after <= sequence),
            exited: state.receipt.is_some(),
            receipt: state.receipt.clone(),
        })
    }
}

impl Drop for OutputLog {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

fn append_frame(path: &Path, chunk: &OutputChunk) -> std::io::Result<()> {
    let mut file = OpenOptions::new().append(true).open(path)?;
    write_frame(&mut file, chunk)
}

fn rewrite_frames(path: &Path, chunks: &VecDeque<OutputChunk>) -> std::io::Result<()> {
    let mut file = File::create(path)?;
    for chunk in chunks {
        write_frame(&mut file, chunk)?;
    }
    file.sync_data()
}

fn write_frame(file: &mut File, chunk: &OutputChunk) -> std::io::Result<()> {
    file.write_all(&chunk.sequence.to_be_bytes())?;
    file.write_all(&[match chunk.stream {
        OutputStream::Stdout => 0,
        OutputStream::Stderr => 1,
    }])?;
    file.write_all(&(chunk.bytes.len() as u32).to_be_bytes())?;
    file.write_all(&chunk.bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_large_write_never_enters_the_live_tail_above_one_mib() {
        let path = std::env::temp_dir().join(format!(
            "layerfs-output-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let log = OutputLog::create(&path).unwrap();
        log.append(OutputStream::Stdout, &vec![7; OUTPUT_TAIL_BYTES * 2])
            .unwrap();
        let page = log.read(0, false).unwrap();
        assert!(page.truncated);
        assert_eq!(
            page.chunks
                .iter()
                .map(|chunk| chunk.bytes.len())
                .sum::<usize>(),
            OUTPUT_TAIL_BYTES
        );
        drop(log);
        assert!(!path.exists());
    }
}
