//! Diagnostic, process-wide totals of the steps of a kernel READ (FUSE
//! profile probe). Each stage keeps a count and summed wall nanoseconds over
//! every thread; nested stages are not subtracted from their parents. A
//! reader diffs [`snapshot`] around the phase it measures.
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

/// One timed step of a kernel READ.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(usize)]
pub enum Stage {
    /// The read-only owner visit of one READ window (overlay job).
    ReadVisit,
    /// Taking the Store reader for the base part of a window.
    BaseAdmit,
    /// Finishing a window from the base: read-ahead plus the planned range.
    Finish,
    /// `CanonicalClient::read_ahead`, inside Finish.
    ReadAhead,
    /// One `CanonicalClient` demand: cache lookup, upstream and checks.
    ClientDemand,
    /// One daemon read-session Store demand, inside ClientDemand/ReadAhead.
    StoreDemand,
    /// Locator lookups in SQLite, inside StoreDemand.
    Locate,
    /// Pack body reads from SQLite (with their SHA-256), inside StoreDemand.
    PackRead,
    /// SHA-256 of a pack body, inside PackRead.
    PackHash,
    /// zstd decompression, inside StoreDemand.
    Zstd,
    /// BLAKE3 object identity checks, anywhere.
    ObjectHash,
    /// The reply handed to the kernel (copy into the FUSE device).
    Reply,
    /// Prototype: the locate SQL statements alone, inside Locate.
    LocateSql,
    /// Prototype: the pack descriptor statement of one pack read, inside PackRead.
    PackDescriptor,
}
/// Number of [`Stage`]s.
pub const STAGES: usize = Stage::PackDescriptor as usize + 1;
/// Names of the [`Stage`]s, in order.
pub const STAGE_NAMES: [&str; STAGES] = [
    "read_visit",
    "base_admit",
    "finish",
    "read_ahead",
    "client_demand",
    "store_demand",
    "locate",
    "pack_read",
    "pack_hash",
    "zstd",
    "object_hash",
    "reply",
    "locate_sql",
    "pack_descriptor",
];

const ZERO: AtomicU64 = AtomicU64::new(0);
static COUNTS: [AtomicU64; STAGES] = [ZERO; STAGES];
static NANOS: [AtomicU64; STAGES] = [ZERO; STAGES];

/// Charges the time since `started` to `stage`.
pub fn charge(stage: Stage, started: Instant) {
    let elapsed = started.elapsed().as_nanos().min(u64::MAX as u128) as u64;
    COUNTS[stage as usize].fetch_add(1, Ordering::Relaxed);
    NANOS[stage as usize].fetch_add(elapsed, Ordering::Relaxed);
}
/// Runs `work` and charges its wall time to `stage`.
pub fn time<T>(stage: Stage, work: impl FnOnce() -> T) -> T {
    let started = Instant::now();
    let value = work();
    charge(stage, started);
    value
}
/// Counts and nanoseconds of every stage so far.
pub fn snapshot() -> ([u64; STAGES], [u64; STAGES]) {
    (
        std::array::from_fn(|index| COUNTS[index].load(Ordering::Relaxed)),
        std::array::from_fn(|index| NANOS[index].load(Ordering::Relaxed)),
    )
}
/// Charges the time from [`Timer::start`] to its drop to one stage.
pub struct Timer(Stage, Instant);
impl Timer {
    /// Starts timing `stage`.
    pub fn start(stage: Stage) -> Self {
        Self(stage, Instant::now())
    }
}
impl Drop for Timer {
    fn drop(&mut self) {
        charge(self.0, self.1);
    }
}

/// Prototype (parallel-store probe): wall time during which at least one
/// Store demand ran, and the most that ran at once. Unlike the summed stage
/// time it does not grow when demands overlap, so busy / wall is a share.
pub mod busy {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::OnceLock;
    use std::time::Instant;

    static EPOCH: OnceLock<Instant> = OnceLock::new();
    static ACTIVE: AtomicU64 = AtomicU64::new(0);
    static SINCE: AtomicU64 = AtomicU64::new(0);
    static BUSY: AtomicU64 = AtomicU64::new(0);
    static PEAK: AtomicU64 = AtomicU64::new(0);

    fn now() -> u64 {
        EPOCH
            .get_or_init(Instant::now)
            .elapsed()
            .as_nanos()
            .min(u64::MAX as u128) as u64
    }
    /// Union busy nanoseconds so far and the peak concurrency so far.
    pub fn snapshot() -> (u64, u64) {
        (BUSY.load(Ordering::Relaxed), PEAK.load(Ordering::Relaxed))
    }
    /// One running Store demand, from [`Demand::enter`] to its drop.
    pub struct Demand(());
    impl Demand {
        /// Marks one more demand running.
        pub fn enter() -> Self {
            let at = now();
            let before = ACTIVE.fetch_add(1, Ordering::AcqRel);
            if before == 0 {
                SINCE.store(at, Ordering::Release);
            }
            PEAK.fetch_max(before + 1, Ordering::Relaxed);
            Self(())
        }
    }
    impl Drop for Demand {
        fn drop(&mut self) {
            let since = SINCE.load(Ordering::Acquire);
            let at = now();
            if ACTIVE.fetch_sub(1, Ordering::AcqRel) == 1 {
                BUSY.fetch_add(at.saturating_sub(since), Ordering::Relaxed);
            }
        }
    }
}
