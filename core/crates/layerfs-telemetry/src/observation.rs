//! Portable process-window arithmetic; none of these values imply exclusive cost.

/// Fixed collection identity, independent of process scope and clock domains.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Source {
    /// Caller-supplied typed data; makes no native collection claim.
    #[default]
    Supplied,
    /// Safe POSIX CPU plus libproc RSS/incarnation on macOS.
    Macos,
    /// Bounded procfs stat with sysconf units on Linux.
    Linux,
}
impl Source {
    /// Stable bounded name in native report schema v1.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Supplied => "supplied",
            Self::Macos => "getrusage+libproc",
            Self::Linux => "procfs+sysconf",
        }
    }
}

/// One process sample in a local monotonic clock domain. None means unavailable.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Observation {
    /// Selected measurements: bit 0 CPU, bit 1 RSS; null fields with selected bits
    /// mean unavailable, while an unset bit means deliberately disabled.
    pub selected: u8,
    /// Collection API identity; synthetic reports remain explicitly supplied.
    pub source: Source,
    /// Observed native collection wall time; absent on supplied data.
    pub probe_ns: Option<u64>,
    /// Local nanoseconds since monitor start.
    pub at_ns: u64,
    /// Native process start identity, stable only within the stated host scope.
    pub incarnation: u64,
    /// Cumulative user CPU nanoseconds.
    pub user_ns: Option<u64>,
    /// Cumulative system CPU nanoseconds.
    pub system_ns: Option<u64>,
    /// Resident bytes, not heap size or a phase peak.
    pub rss: Option<u64>,
}
/// Constant-size aggregate independent of recent sample eviction.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Window {
    /// Local monitor time when the operation window was opened, if observed.
    pub opened_ns: Option<u64>,
    /// Local monitor time when the window closed, if observed.
    pub closed_ns: Option<u64>,
    /// First observation, whose timestamp states actual boundary coverage.
    pub first: Option<Observation>,
    /// Last observation.
    pub last: Option<Observation>,
    /// Observed sampled maximum RSS, never an exact maximum.
    pub sampled_max_rss: Option<u64>,
    /// Samples received.
    pub samples: u64,
    /// Unavailable or skipped samples.
    pub gaps: u64,
    /// Largest observed sample gap.
    pub largest_gap_ns: u64,
    /// Maximum simultaneous active windows; observations include sibling work.
    pub concurrency: usize,
    /// Counter decrease or process incarnation change invalidates CPU deltas.
    pub discontinuity: bool,
}
impl Window {
    /// Adds one observation without retaining a per-sample collection.
    pub fn observe(&mut self, s: Observation, concurrency: usize) {
        if let Some(last) = self.last {
            if s.at_ns < last.at_ns
                || s.incarnation != last.incarnation
                || s.source != last.source
                || s.selected != last.selected
                || matches!((last.user_ns,s.user_ns),(Some(a),Some(b)) if b<a)
                || matches!((last.system_ns,s.system_ns),(Some(a),Some(b)) if b<a)
            {
                self.discontinuity = true;
            }
            self.largest_gap_ns = self.largest_gap_ns.max(s.at_ns.saturating_sub(last.at_ns));
        }
        self.first.get_or_insert(s);
        self.last = Some(s);
        if self.samples == u64::MAX {
            self.discontinuity = true;
        }
        self.samples = self.samples.saturating_add(1);
        self.concurrency = self.concurrency.max(concurrency);
        if let Some(rss) = s.rss {
            self.sampled_max_rss = Some(self.sampled_max_rss.map_or(rss, |old| old.max(rss)));
        }
        if s.selected & 1 != 0 && (s.user_ns.is_none() || s.system_ns.is_none()) {
            self.gaps = self.gaps.saturating_add(1);
        }
    }
    /// Covered cumulative user/system CPU deltas, invalid on gaps/discontinuity.
    pub fn cpu_delta(&self) -> Option<(u64, u64)> {
        if self.discontinuity || self.gaps != 0 || self.samples < 2 {
            return None;
        }
        let first = self.first?;
        let last = self.last?;
        if last.at_ns <= first.at_ns || first.selected & 1 == 0 || last.selected & 1 == 0 {
            return None;
        }
        Some((
            last.user_ns?.checked_sub(first.user_ns?)?,
            last.system_ns?.checked_sub(first.system_ns?)?,
        ))
    }
}
/// One optional observation boundary, supplied by enabled assembly.
/// Implementations must use bounded nonblocking acquisition and atomic release.
pub trait WindowSource: Send + Sync {
    /// Tries to reserve a generation-checked active slot.
    fn begin(&self) -> Option<u64>;
    /// Tries to copy its constant-size summary. Does not release the slot.
    fn finish(&self, token: u64) -> Option<Window>;
    /// Bounded release on normal return and unwind; no output or worker joins.
    fn release(&self, token: u64);
}
