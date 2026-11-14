//! Diagnostic, process-wide totals of the steps of a kernel READ and of a
//! kernel mutation (FUSE profile probe). Each stage keeps a count and summed wall nanoseconds over
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
    /// Prototype (write-path probe): one mutation owner visit as the request
    /// sees it, dispatch and the daemon's post-job step included.
    MutateVisit,
    /// Base facts read between undecided mutation visits.
    MutateSupply,
    /// The write recognizer (claims of committed chunks), before the visit.
    Recognize,
    /// The post-reply release job of a published mutation's ticket.
    MutateRelease,
    /// The reply of a mutation handed to the kernel.
    MutateReply,
    /// Waiting for the Workspace lock before an owner job (any class).
    OwnerWait,
    /// One owner job under the Workspace lock (any class).
    OwnerHold,
    /// Maintenance drained on the caller's thread after an owner job.
    OwnerMaintain,
    /// `Overlay::mutate_kernel_visit`: fence, decision, apply and COMMIT.
    VisitOverlay,
    /// The Workspace decision inside a mutation visit (publication included).
    VisitDecide,
    /// `prepare_kernel_mutation` inside the decision.
    VisitPublish,
    /// Overlay statements of owner jobs by family (count = statements):
    /// BEGIN IMMEDIATE.
    SqlBegin,
    /// COMMIT.
    SqlCommit,
    /// Inode rows.
    SqlInode,
    /// Directory entries.
    SqlEntry,
    /// Payload cells and pieces.
    SqlPayload,
    /// Frontier and publication rows.
    SqlFrontier,
    /// Operation records.
    SqlRecord,
    /// Reclamation.
    SqlReclaim,
    /// Startup, capture, explain and rollback.
    SqlOther,
    /// LOOKUP/GETATTR/OPEN/OPENDIR owner visit as the request sees it.
    ObserveVisit,
    /// Base facts read between undecided observe visits.
    ObserveSupply,
    /// Owner lock waits longer than 5 µs (count and their time).
    OwnerWaitLong,
    /// Overlay visit fence and source, before the decision.
    OverlayFence,
    /// `check_changes` of a mutation.
    OverlayCheck,
    /// Inode rows written by `apply_checked`.
    OverlayInodes,
    /// Payload cells of a write (`write_cells`/`write_claimed`), SQL and BLOB I/O.
    OverlayCells,
    /// BLAKE3 of an appended write followed as a whole-content digest (#364).
    OverlayDigest,
    /// BLAKE3 of an appended window on the digest helper thread (prototype).
    OverlayDigestHelper,
    /// `digestcheck`: helper digests equal to an inline update (count).
    DigestCheckSame,
    /// `digestcheck`: helper digests that differ from an inline update.
    DigestCheckDiffer,
    /// The rest of `apply_checked` after the cells: digests and settle.
    OverlaySettle,
    /// `kernel_effect` after apply (reply ticket, kernel custody rows).
    OverlayEffect,
    /// A GETATTR reply of a directory (counts only).
    GetattrDirectory,
    /// A GETATTR reply of a non-directory.
    GetattrFile,
    /// A GETATTR that named a descriptor.
    GetattrHandle,
    /// A GETATTR of the parent the latest CREATE/MKDIR named.
    GetattrCreateParent,
    /// A GETATTR answered from the published-attribute cache (prototype).
    GetattrCached,
}
/// Number of [`Stage`]s.
pub const STAGES: usize = Stage::GetattrCached as usize + 1;
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
    "mutate_visit",
    "mutate_supply",
    "recognize",
    "mutate_release",
    "mutate_reply",
    "owner_wait",
    "owner_hold",
    "owner_maintain",
    "visit_overlay",
    "visit_decide",
    "visit_publish",
    "sql_begin",
    "sql_commit",
    "sql_inode",
    "sql_entry",
    "sql_payload",
    "sql_frontier",
    "sql_record",
    "sql_reclaim",
    "sql_other",
    "observe_visit",
    "observe_supply",
    "owner_wait_long",
    "overlay_fence",
    "overlay_check",
    "overlay_inodes",
    "overlay_cells",
    "overlay_digest",
    "overlay_digest_helper",
    "digest_check_same",
    "digest_check_differ",
    "overlay_settle",
    "overlay_effect",
    "getattr_directory",
    "getattr_file",
    "getattr_handle",
    "getattr_create_parent",
    "getattr_cached",
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
/// Adds `count` and `nanos` to `stage` (work measured elsewhere).
pub fn add(stage: Stage, count: u64, nanos: u64) {
    COUNTS[stage as usize].fetch_add(count, Ordering::Relaxed);
    NANOS[stage as usize].fetch_add(nanos, Ordering::Relaxed);
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
