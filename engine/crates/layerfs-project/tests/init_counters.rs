//! Init's live counts end at what it imported: every source entry, and each
//! regular-file identity once with its bytes.
mod support;
use layerfs_history::{HistoryName, LayerStackId};
use layerfs_project::{init_observed, InitCounters, InitCounts, InitRequest, InitStep};
use layerfs_telemetry::timer::Timing;
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use support::*;

#[test]
fn init_counts_every_entry_and_each_file_identity_once() {
    let fixture = Fixture::new(100);
    // a second path of one identity: an entry, but not a second file
    std::fs::hard_link(
        fixture.source.join("d0/f0000"),
        fixture.source.join("d0/link"),
    )
    .unwrap();
    let storage = storage(Arc::new(memory_metadata::MemoryMetadata::default()));
    let history = memory_history::MemoryHistory::default();
    let acquisition = memory_acquisition::MemoryAcquisition::default();
    let counters = Arc::new(InitCounters::new());
    assert_eq!(counters.read().step, InitStep::Scan);
    let initialized = Timing::disabled("project.init", |scope| {
        init_observed(
            &storage,
            &history,
            InitRequest {
                source: &fixture.source,
                acquisition: &acquisition,
                stack: LayerStackId::from_authority([19; 16]),
                name: HistoryName::new("main").unwrap(),
                scope_seed: [29; 32],
                deadline: Instant::now() + Duration::from_secs(60),
            },
            scope,
            Arc::clone(&counters),
        )
    })
    .0
    .unwrap();
    let bytes = fixture
        .expected
        .values()
        .map(|body| body.len() as u64)
        .sum();
    // the root, ten directories, 100 files and the link
    assert_eq!(initialized.entries, 112);
    assert_eq!(
        counters.read(),
        InitCounts {
            step: InitStep::Publish,
            entries: 112,
            files: 100,
            bytes,
        }
    );
}
