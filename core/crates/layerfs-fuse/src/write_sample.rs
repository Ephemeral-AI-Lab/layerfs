//! Optional, bounded write-path counter snapshots for operators.
use layerfs_workspace::Workspace;
use std::{
    sync::atomic::{AtomicU64, Ordering},
    time::Instant,
};

pub(crate) struct WriteSamples {
    interval: Option<u64>,
    start: Instant,
    acquisition_ns: AtomicU64,
    publication_ns: AtomicU64,
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
        }
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
        eprintln!(
            "LFS_WRITE_SAMPLE v=2 write_class={count:?} elapsed_ns={} backing={:?} metadata={:?} acquisition_ns={} publication_ns={}",
            self.start.elapsed().as_nanos(), workspace.backing_status(), workspace.metadata_status(),
            self.acquisition_ns.load(Ordering::Relaxed), self.publication_ns.load(Ordering::Relaxed)
        );
    }
}
