//! Bounded per-operation counts of projection callbacks and upstream calls.
//!
//! These are ordinary product telemetry: they let an operator see how much
//! kernel and host work a mounted Workspace is actually doing, so avoidable
//! repeated metadata lookups or upstream requests can be identified from a
//! real route instead of guessed at. Counts are saturating and never gate an
//! operation.

/// One more than the largest request the projection distinguishes.
///
/// A transfer larger than the last bucket is counted in the last bucket, so a
/// reader sees "at least this size" rather than an unbounded table.
pub const SIZE_BUCKETS: usize = 18;

/// Saturating histogram of one projection request-size distribution.
///
/// Bucket `0` holds sizes `0..=1`; bucket `i` holds `2^(i-1)+1..=2^i`; the last
/// bucket also holds every larger size. The table is fixed, so a read is
/// bounded and the reported shape is stable across kernel versions.
#[derive(Clone, Copy, Default)]
pub struct SizeHistogram {
    buckets: [u64; SIZE_BUCKETS],
}

impl SizeHistogram {
    /// Records one request size. Never fails and never gates the callback.
    pub fn record(&mut self, size: u64) {
        let index = if size <= 1 {
            0
        } else {
            (u64::BITS - (size - 1).leading_zeros()) as usize
        };
        let slot = &mut self.buckets[index.min(SIZE_BUCKETS - 1)];
        *slot = slot.saturating_add(1);
    }

    pub fn count(&self, bucket: usize) -> u64 {
        self.buckets.get(bucket).copied().unwrap_or(0)
    }

    pub fn buckets(&self) -> [u64; SIZE_BUCKETS] {
        self.buckets
    }

    /// The stable labels of every bucket, in `buckets` order.
    pub const LABELS: [&'static str; SIZE_BUCKETS] = [
        "0-1", "2", "3-4", "5-8", "9-16", "17-32", "33-64", "65-128", "129-256", "257-512",
        "513-1k", "1k-2k", "2k-4k", "4k-8k", "8k-16k", "16k-32k", "32k-64k", "64k-128k",
    ];
}

/// Saturating request and transferred byte totals for the data callbacks.
///
/// `request` is what the kernel asked the projection to move and `returned` is
/// what the projection actually moved, so a short transfer is visible instead
/// of being averaged away. The histogram classifies the request sizes, which is
/// what distinguishes a large request answered short from a large transfer the
/// caller split into smaller requests.
#[derive(Clone, Copy, Default)]
pub struct ProjectionBytes {
    pub read_request: u64,
    pub read_returned: u64,
    pub read_sizes: SizeHistogram,
    pub write_request: u64,
    pub write_returned: u64,
    pub write_sizes: SizeHistogram,
}

/// The stable labels of the bounded byte totals, in reporting order.
pub const PROJECTION_BYTE_LABELS: [&str; 4] = [
    "read_request",
    "read_returned",
    "write_request",
    "write_returned",
];

/// The projection callbacks this build distinguishes.
///
/// The set is fixed and small so a counter read is bounded and the reported
/// shape is stable. `Other` covers the remaining callbacks without growing the
/// table per kernel version.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProjectionOp {
    Lookup,
    Getattr,
    Read,
    Write,
    Readdir,
    Open,
    Setattr,
    Rename,
    Other,
}

impl ProjectionOp {
    pub const ALL: [ProjectionOp; 9] = [
        ProjectionOp::Lookup,
        ProjectionOp::Getattr,
        ProjectionOp::Read,
        ProjectionOp::Write,
        ProjectionOp::Readdir,
        ProjectionOp::Open,
        ProjectionOp::Setattr,
        ProjectionOp::Rename,
        ProjectionOp::Other,
    ];

    fn index(self) -> usize {
        match self {
            ProjectionOp::Lookup => 0,
            ProjectionOp::Getattr => 1,
            ProjectionOp::Read => 2,
            ProjectionOp::Write => 3,
            ProjectionOp::Readdir => 4,
            ProjectionOp::Open => 5,
            ProjectionOp::Setattr => 6,
            ProjectionOp::Rename => 7,
            ProjectionOp::Other => 8,
        }
    }

    /// The stable name used by status readers.
    pub fn label(self) -> &'static str {
        match self {
            ProjectionOp::Lookup => "lookup",
            ProjectionOp::Getattr => "getattr",
            ProjectionOp::Read => "read",
            ProjectionOp::Write => "write",
            ProjectionOp::Readdir => "readdir",
            ProjectionOp::Open => "open",
            ProjectionOp::Setattr => "setattr",
            ProjectionOp::Rename => "rename",
            ProjectionOp::Other => "other",
        }
    }
}

/// Saturating counts of projection callbacks and upstream host calls.
///
/// A count never wraps and never fails an operation. `upstream` counts calls
/// the Workspace sent to the host Service, which is the quantity a transport
/// change is expected to move.
#[derive(Default)]
pub struct ProjectionCounters {
    callbacks: [u64; ProjectionOp::ALL.len()],
    upstream: u64,
    bytes: ProjectionBytes,
}

impl ProjectionCounters {
    pub fn record(&mut self, op: ProjectionOp) {
        let slot = &mut self.callbacks[op.index()];
        *slot = slot.saturating_add(1);
    }

    pub fn record_upstream(&mut self) {
        self.upstream = self.upstream.saturating_add(1);
    }

    /// Records one data callback's request and transferred byte counts.
    ///
    /// A short transfer is ordinary: the callback reports both numbers so a
    /// reader can tell it apart from a caller that asked for less.
    pub fn record_bytes(&mut self, op: ProjectionOp, request: u64, returned: u64) {
        match op {
            ProjectionOp::Read => {
                self.bytes.read_request = self.bytes.read_request.saturating_add(request);
                self.bytes.read_returned = self.bytes.read_returned.saturating_add(returned);
                self.bytes.read_sizes.record(request);
            }
            ProjectionOp::Write => {
                self.bytes.write_request = self.bytes.write_request.saturating_add(request);
                self.bytes.write_returned = self.bytes.write_returned.saturating_add(returned);
                self.bytes.write_sizes.record(request);
            }
            _ => {}
        }
    }

    pub fn count(&self, op: ProjectionOp) -> u64 {
        self.callbacks[op.index()]
    }

    pub fn upstream(&self) -> u64 {
        self.upstream
    }

    pub fn bytes(&self) -> ProjectionBytes {
        self.bytes
    }

    /// The bounded byte totals in `PROJECTION_BYTE_LABELS` order.
    pub fn byte_totals(&self) -> [u64; PROJECTION_BYTE_LABELS.len()] {
        [
            self.bytes.read_request,
            self.bytes.read_returned,
            self.bytes.write_request,
            self.bytes.write_returned,
        ]
    }
}
