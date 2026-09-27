//! Fixed-size operator counters for local metadata ownership work.
use crate::{runtime::host::AtomicTimedCount, MetadataStatus};
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Default)]
pub(crate) struct ArenaTelemetry {
    pub ledger_file: AtomicTimedCount,
    pub metadata_page_create: AtomicTimedCount,
    pub sponsor_attempts: AtomicU64,
    pub sponsor_accepted: AtomicU64,
    pub sponsor_depth_fallbacks: AtomicU64,
}

impl ArenaTelemetry {
    pub(super) fn add_to(&self, status: &mut MetadataStatus) {
        let (calls, ns) = self.ledger_file.snapshot();
        status.ledger_file_calls = status.ledger_file_calls.saturating_add(calls);
        status.ledger_file_ns = status.ledger_file_ns.saturating_add(ns);
        let (calls, ns) = self.metadata_page_create.snapshot();
        status.metadata_page_create_calls = status.metadata_page_create_calls.saturating_add(calls);
        status.metadata_page_create_ns = status.metadata_page_create_ns.saturating_add(ns);
        status.sponsor_attempts = status
            .sponsor_attempts
            .saturating_add(self.sponsor_attempts.load(Ordering::Relaxed));
        status.sponsor_accepted = status
            .sponsor_accepted
            .saturating_add(self.sponsor_accepted.load(Ordering::Relaxed));
        status.sponsor_depth_fallbacks = status
            .sponsor_depth_fallbacks
            .saturating_add(self.sponsor_depth_fallbacks.load(Ordering::Relaxed));
    }
}
