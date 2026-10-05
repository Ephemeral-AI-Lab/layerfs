//! Optional operation recording with ownership held through report consumption.

use crate::observation::{Window, WindowSource};
use crate::timer::{Active, RecordingLimits, Timing, TimingReport, TimingScope};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

/// Finite shared recording slots. Disabled construction allocates nothing.
#[derive(Clone, Default)]
pub struct OperationRecorder {
    pool: Option<Arc<Pool>>,
    windows: Option<Arc<dyn WindowSource>>,
    timing_enabled: bool,
}
struct Pool {
    used: AtomicUsize,
    slots: usize,
    limits: RecordingLimits,
}
struct Lease(Arc<Pool>);
impl Drop for Lease {
    fn drop(&mut self) {
        self.0.used.fetch_sub(1, Ordering::Release);
    }
}

/// Independent optional diagnostic outcome; it never replaces the product result.
pub enum Diagnostic {
    /// No observations or allocations were requested.
    Disabled,
    /// The shared ownership budget had no available slot.
    Omitted,
    /// Bounded completed data; its reservation lives until this value is dropped.
    Report(Box<OperationReport>),
}

/// One locally timed operation, never a cross-process elapsed interval.
pub struct OperationReport {
    key: u64,
    success: bool,
    timing: TimingReport,
    _lease: Lease,
    window: Option<Window>,
    resource_requested: bool,
}
impl OperationReport {
    /// Distinguishes unselected resources, omitted windows and unavailable samples.
    pub fn resource_status(&self) -> &'static str {
        if !self.resource_requested {
            "disabled"
        } else {
            match self.window {
                None => "omitted",
                Some(w) if w.samples == 0 => "unavailable",
                Some(_) => "sampled",
            }
        }
    }

    /// Original product outcome, present even when timing was deselected.
    pub const fn success(&self) -> bool {
        self.success
    }
    /// Shared process observations and actual sample coverage.
    pub fn window(&self) -> Option<&Window> {
        self.window.as_ref()
    }
    /// Correlation key supplied by assembly, not an authority or replay key.
    pub const fn key(&self) -> u64 {
        self.key
    }
    /// Borrow completed data without cloning away its ownership reservation.
    pub fn timing(&self) -> &TimingReport {
        &self.timing
    }
}
impl OperationRecorder {
    /// Master-off: no allocation, clock, worker or output.
    pub const fn disabled() -> Self {
        Self {
            pool: None,
            windows: None,
            timing_enabled: false,
        }
    }
    /// Constructs an explicitly enabled recorder with checked count and byte caps.
    /// Two per-recording charges reserve live/conversion/import overlap. Incoming
    /// reports and explicit caller clones remain charged to the caller's pool.
    pub fn new(limits: RecordingLimits, slots: usize, bytes: usize) -> Option<Self> {
        if slots == 0
            || slots > 32
            || slots.checked_mul(limits.charge())?.checked_mul(2)? > bytes
            || bytes > 4 * 1024 * 1024
        {
            return None;
        }
        Some(Self {
            windows: None,
            timing_enabled: true,
            pool: Some(Arc::new(Pool {
                used: AtomicUsize::new(0),
                slots,
                limits,
            })),
        })
    }
    /// Selects optional resources on an already enabled recorder. A disabled
    /// recorder never retains or calls the supplied resource source.
    pub fn with_windows(mut self, source: Arc<dyn WindowSource>) -> Self {
        if self.pool.is_some() {
            self.windows = Some(source);
        }
        self
    }
    /// Independently disables timing while preserving selected process windows.
    pub fn with_timing(mut self, enabled: bool) -> Self {
        self.timing_enabled = enabled;
        self
    }
    /// Runs exactly once, preserving the original Result and panic semantics.
    /// Capacity refusal disables detail only. Drop releases a slot without I/O.
    pub fn run<T, E, F>(
        &self,
        key: u64,
        label: &'static str,
        operation: F,
    ) -> (Result<T, E>, Diagnostic)
    where
        F: FnOnce(&TimingScope<'_, Active>) -> Result<T, E>,
    {
        let Some(pool) = &self.pool else {
            return (Timing::disabled(label, operation).0, Diagnostic::Disabled);
        };
        if pool
            .used
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |used| {
                (used < pool.slots).then_some(used + 1)
            })
            .is_err()
        {
            return (Timing::disabled(label, operation).0, Diagnostic::Omitted);
        }
        let lease = Lease(Arc::clone(pool));
        let window_lease = self.windows.as_ref().and_then(|source| {
            source.begin().map(|token| WindowLease {
                source: source.clone(),
                token,
            })
        });
        let (result, timing) = if self.timing_enabled {
            Timing::record_with_limits(pool.limits, label, operation)
        } else {
            Timing::disabled(label, operation)
        };
        let window = window_lease
            .as_ref()
            .and_then(|lease| lease.source.finish(lease.token));
        drop(window_lease);
        let success = result.is_ok();
        (
            result,
            Diagnostic::Report(Box::new(OperationReport {
                key,
                success,
                timing,
                _lease: lease,
                window,
                resource_requested: self.windows.is_some(),
            })),
        )
    }
}

struct WindowLease {
    source: Arc<dyn WindowSource>,
    token: u64,
}
impl Drop for WindowLease {
    fn drop(&mut self) {
        self.source.release(self.token);
    }
}
