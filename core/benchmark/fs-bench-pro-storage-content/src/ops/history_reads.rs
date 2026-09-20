//! Aggregate provider demand without retaining IDs, bytes, or per-read timer nodes.
use std::cell::Cell;
use std::time::Instant;

use layerfs_content::{AuthenticatedObjects, ContentResult, ObjectId};
use layerfs_telemetry::timer::TimingScope;

/// Counts at the provider boundary, not physical disk traffic or decoded pack work.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ReadCounters {
    /// Provider batch calls attempted, including empty demands and failures.
    pub waves: u64,
    /// Requested IDs summed across calls, including repeated IDs.
    pub requested_objects: u64,
    /// Canonical values returned by successful calls.
    pub returned_objects: u64,
    /// Canonical bytes returned by successful calls; not disk or pack bytes.
    pub returned_bytes: u64,
    /// Calls returning the original provider error.
    pub failed_waves: u64,
    /// Delegated call wall time, overlapping the enclosing operation phase.
    pub elapsed_ns: u64,
}

/// Forwards the exact provider method and result; collection is explicitly opt-in.
pub struct ReadWork<P> {
    /// Original provider, retaining its public operation counters.
    pub inner: P,
    enabled: bool,
    counters: Cell<ReadCounters>,
}

impl<P> ReadWork<P> {
    /// Wraps a provider; disabled observation forwards without reading the clock.
    pub fn new(inner: P, enabled: bool) -> Self {
        Self {
            inner,
            enabled,
            counters: Cell::new(ReadCounters::default()),
        }
    }

    /// Returns the aggregate counters without resetting them.
    pub fn counters(&self) -> ReadCounters {
        self.counters.get()
    }

    fn measure(
        &self,
        ids: &[ObjectId],
        read: impl FnOnce() -> ContentResult<Vec<Vec<u8>>>,
    ) -> ContentResult<Vec<Vec<u8>>> {
        if !self.enabled {
            return read();
        }
        let start = Instant::now();
        let result = read();
        let elapsed_ns = start.elapsed().as_nanos() as u64;
        let mut counters = self.counters.get();
        counters.waves += 1;
        counters.requested_objects += ids.len() as u64;
        counters.elapsed_ns += elapsed_ns;
        match &result {
            Ok(values) => {
                counters.returned_objects += values.len() as u64;
                counters.returned_bytes +=
                    values.iter().map(|value| value.len() as u64).sum::<u64>();
            }
            Err(_) => counters.failed_waves += 1,
        }
        self.counters.set(counters);
        result
    }
}

impl<P: AuthenticatedObjects> AuthenticatedObjects for ReadWork<P> {
    fn read_canonical_batch(&self, ids: &[ObjectId]) -> ContentResult<Vec<Vec<u8>>> {
        self.measure(ids, || self.inner.read_canonical_batch(ids))
    }

    fn read_canonical_batch_scoped(
        &self,
        ids: &[ObjectId],
        scope: TimingScope<'_>,
    ) -> ContentResult<Vec<Vec<u8>>> {
        self.measure(ids, || self.inner.read_canonical_batch_scoped(ids, scope))
    }
}
