//! Optional, bounded write-path counter snapshots for operators.
use layerfs_workspace::Workspace;
use std::{sync::OnceLock, time::Instant};

pub(crate) fn record(workspace: &Workspace) {
    static INTERVAL: OnceLock<Option<u64>> = OnceLock::new();
    static START: OnceLock<Instant> = OnceLock::new();
    let Some(interval) = *INTERVAL.get_or_init(|| {
        std::env::var("LAYERFS_FUSE_WRITE_SAMPLE_INTERVAL")
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .filter(|value| *value > 0)
    }) else {
        return;
    };
    let started = START.get_or_init(Instant::now);
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
        "LFS_WRITE_SAMPLE v=1 write_class={count:?} elapsed_ns={} backing={:?} metadata={:?}",
        started.elapsed().as_nanos(),
        workspace.backing_status(),
        workspace.metadata_status()
    );
}
