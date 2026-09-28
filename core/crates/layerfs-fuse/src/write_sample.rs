//! Optional, bounded write-path counter snapshots for operators.
use fuser::Errno;
use layerfs_workspace::{Workspace, WorkspaceError};
use std::{
    fmt::{self, Write as _},
    io::Write as _,
    sync::atomic::{AtomicU64, Ordering},
    time::Instant,
};

// One fixed-size operator observation. No heap allocation, replacement
// cache, or unbounded formatted line is retained across callbacks.
const SAMPLE_LINE_BYTES: usize = 4096;

struct SampleLine {
    bytes: [u8; SAMPLE_LINE_BYTES],
    len: usize,
}

impl SampleLine {
    fn new() -> Self {
        Self {
            bytes: [0; SAMPLE_LINE_BYTES],
            len: 0,
        }
    }

    fn bytes(&self) -> &[u8] {
        &self.bytes[..self.len]
    }
}

impl fmt::Write for SampleLine {
    fn write_str(&mut self, value: &str) -> fmt::Result {
        let end = self.len.checked_add(value.len()).ok_or(fmt::Error)?;
        let out = self.bytes.get_mut(self.len..end).ok_or(fmt::Error)?;
        out.copy_from_slice(value.as_bytes());
        self.len = end;
        Ok(())
    }
}

pub(crate) struct WriteSamples {
    interval: Option<u64>,
    start: Instant,
    acquisition_ns: AtomicU64,
    publication_ns: AtomicU64,
    refusals: AtomicU64,
}

impl WriteSamples {
    pub(crate) fn new() -> Self {
        let interval = std::env::var("LAYERFS_FUSE_WRITE_SAMPLE_INTERVAL")
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .filter(|value| *value > 0);
        Self {
            interval,
            start: Instant::now(),
            acquisition_ns: AtomicU64::new(0),
            publication_ns: AtomicU64::new(0),
            refusals: AtomicU64::new(0),
        }
    }

    pub(crate) fn map_error(
        &self,
        workspace: &Workspace,
        stage: &str,
        error: WorkspaceError,
    ) -> Errno {
        if matches!(&error, WorkspaceError::Busy)
            && self
                .refusals
                .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| {
                    (n < 8).then_some(n + 1)
                })
                .is_ok()
        {
            let status = workspace.status();
            eprintln!("LFS_WRITE_REFUSAL v=1 stage={stage} status={status:?}");
        }
        crate::replies::errno(error)
    }

    pub(crate) fn record(&self, workspace: &Workspace, acquisition_ns: u64, publication_ns: u64) {
        let Some(interval) = self.interval else {
            return;
        };
        let add = |total: &AtomicU64, value| {
            let _ = total.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |old| {
                Some(old.saturating_add(value))
            });
        };
        add(&self.acquisition_ns, acquisition_ns);
        add(&self.publication_ns, publication_ns);
        let status = workspace.status();
        let count = status.as_ref().ok().and_then(|value| {
            value
                .projection_calls
                .iter()
                .find_map(|(name, count)| (*name == "write").then_some(*count))
        });
        if !count.is_some_and(|value| value % interval == 0) {
            return;
        }
        let mut line = SampleLine::new();
        if writeln!(
            &mut line,
            "LFS_WRITE_SAMPLE v=4 write_class={count:?} elapsed_ns={} backing={:?} metadata={:?} acquisition_ns={} publication_ns={}",
            self.start.elapsed().as_nanos(), workspace.backing_status(), workspace.metadata_status(),
            self.acquisition_ns.load(Ordering::Relaxed), self.publication_ns.load(Ordering::Relaxed)
        ).is_err() {
            let _ = std::io::stderr().lock().write_all(
                b"LFS_WRITE_SAMPLE v=4 status=INCOMPLETE_OVERFLOW\n"
            );
        } else {
            // One sub-PIPE_BUF write; independent LFT1 output cannot split
            // this record into an apparently complete phase checkpoint.
            let _ = std::io::stderr().lock().write_all(line.bytes());
        }
    }
}
