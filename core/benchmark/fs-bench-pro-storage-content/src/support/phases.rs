//! The declared phase clock: what each of the four phases cost, in the child.
//!
//! `benchmark_rules.md` section 6 requires setup, performance, verification and
//! cleanup to use separate timing and resource scopes. Until round 5 the harness
//! published only two of them, and it published the second one as the *complete
//! command wall*: preparation and verification were billed to a performance
//! budget, and the report's time column was not operation time at all. This module
//! is the missing instrument.
//!
//! **What it is not.** It is harness instrumentation, not product telemetry. It
//! never enters the product's timing tree, it never wraps the measured operation
//! (the product's own `Timing::record` does that, and its root `elapsed_ns` stays
//! the golden number), and it adds no counter, hook or accessor to
//! `core/crates/*/src`. Every mark here is an observation of a boundary the driver
//! already had.
//!
//! **The boundary is where the driver already puts it.** A driver's shape is
//! `build the fixture -> Timing::record(the operation) -> the oracle`, so the
//! preparation phase ends where the measured region opens and the verification
//! phase begins where it closes. [`crate::ops::measure`] is that choke point, and
//! every driver reaches it: a row that measured something cannot avoid marking its
//! own phases, which is what keeps the split honest rather than declared.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::time::Instant;

/// The spans one invocation publishes, in nanoseconds.
///
/// `invocation_ns` is the child's own wall from its first mark to its snapshot.
/// The other six are the declared phases, and they must sum to no more than it:
/// that inequality, checked by `runner.py verify`, is what stops measured work
/// from being moved outside a timer and reported as a saving.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Phases {
    /// Setup: build the fixture, or copy and de-warm a prepared one.
    ///
    /// A driver whose input assembly happens **between** its measured children
    /// attributes those spans with [`add_preparation`], because they are real
    /// invocation time that no other phase accounts for. For such a driver this
    /// field is the close-of-preparation mark **plus** those spans, so it is not
    /// contiguous time; the driver publishes the attributed part separately.
    pub preparation_ns: u64,
    /// The per-sample copy and de-warm inside preparation, published separately
    /// because owner decision D2 makes the acquisition cost mandatory.
    pub acquisition_ns: u64,
    /// The measured operation, from the product's own timing root.
    pub operation_ns: u64,
    /// The oracle: a second, unmeasured, byte-identical operation and its read-back.
    pub verification_ns: u64,
    /// Teardown: destroy the per-case copy, close.
    pub cleanup_ns: u64,
    /// Harness work **inside** the measured region, so it is visible rather than
    /// absorbed into the golden number.
    pub handoff_ns: u64,
    /// The child's whole wall for this invocation.
    pub invocation_ns: u64,
    /// Bytes of the product's own `timing.json` this invocation wrote, or zero
    /// when it wrote none.
    pub timing_json_bytes: u64,
    /// User-mode CPU the **measured region** consumed, from `getrusage`.
    ///
    /// The axis the resource document names fourth and the harness published
    /// nowhere until round 5b: every receipt carried a heap reading and no CPU one.
    /// Both `getrusage` reads happen at the phase boundary — before the timer opens
    /// and after it closes — so the instrument is not inside the region it measures.
    pub cpu_user_ns: u64,
    /// System-mode CPU the **measured region** consumed.
    pub cpu_system_ns: u64,
    /// Peak resident set of this child, from `getrusage(RUSAGE_SELF).ru_maxrss`.
    ///
    /// A **lifetime** figure for the whole process, not a phase number, and labelled
    /// as one: `AGENTS.md` section 5 forbids quoting a lifetime counter as a phase
    /// reading. One child runs one case, so it is that case's peak — a bound and an
    /// anomaly detector, which is what the counted allocator's
    /// `heap.peak_incremental_bytes` is the precise phase figure beside.
    pub process_peak_rss_bytes: u64,
}

impl Phases {
    /// The declared phases, summed. It excludes `handoff_ns`, which is a subset of
    /// `operation_ns` rather than a sibling of it.
    pub fn declared_ns(&self) -> u64 {
        self.preparation_ns + self.operation_ns + self.verification_ns + self.cleanup_ns
    }

    /// The part of the invocation no declared phase accounts for: process start,
    /// the trace header, gate assembly and process teardown.
    pub fn unaccounted_ns(&self) -> u64 {
        self.invocation_ns.saturating_sub(self.declared_ns())
    }

    /// Fields as a flat document, for `phases-<phase>.json`.
    pub fn as_fields(&self) -> Vec<(&'static str, u64)> {
        vec![
            ("preparation_ns", self.preparation_ns),
            ("acquisition_ns", self.acquisition_ns),
            ("operation_ns", self.operation_ns),
            ("verification_ns", self.verification_ns),
            ("cleanup_ns", self.cleanup_ns),
            ("handoff_ns", self.handoff_ns),
            ("invocation_ns", self.invocation_ns),
            ("timing_json_bytes", self.timing_json_bytes),
            ("cpu_user_ns", self.cpu_user_ns),
            ("cpu_system_ns", self.cpu_system_ns),
            ("process_peak_rss_bytes", self.process_peak_rss_bytes),
        ]
    }
}

struct Clock {
    start: Instant,
    output: PathBuf,
    prepared: Option<Instant>,
    measured: Option<Instant>,
    verified: Option<Instant>,
    acquisition_ns: u64,
    /// Harness input assembly attributed to preparation by [`add_preparation`].
    preparation_added_ns: u64,
    operation_ns: u64,
    timing_json_bytes: u64,
    /// CPU at the close of preparation, and at the close of the measured region.
    cpu_at_prepared: Option<crate::support::instruments::CpuReading>,
    cpu_at_measured: Option<crate::support::instruments::CpuReading>,
    process_peak_rss_bytes: u64,
}

static CLOCK: OnceLock<Mutex<Clock>> = OnceLock::new();

/// Whether the measured region is open right now.
///
/// The handoff accounting sits on a path a measured operation calls once per
/// object, so it is kept off the mutex: an atomic load, an atomic add and nothing
/// else. Taking the phase lock 110,022 times would make the instrument the cost it
/// exists to report.
static MEASURED_OPEN: AtomicBool = AtomicBool::new(false);
/// Harness work observed **inside** the measured region.
static HANDOFF_NS: AtomicU64 = AtomicU64::new(0);

fn clock() -> MutexGuard<'static, Clock> {
    CLOCK
        .get_or_init(|| {
            Mutex::new(Clock {
                start: Instant::now(),
                output: PathBuf::new(),
                prepared: None,
                measured: None,
                verified: None,
                acquisition_ns: 0,
                preparation_added_ns: 0,
                operation_ns: 0,
                timing_json_bytes: 0,
                cpu_at_prepared: None,
                cpu_at_measured: None,
                process_peak_rss_bytes: 0,
            })
        })
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Starts the clock for one invocation and registers where it writes.
///
/// The child runs exactly one invocation per process, so a process-global clock
/// is one row's clock and cannot be shared.
pub fn begin(output: &Path) {
    let mut held = clock();
    held.start = Instant::now();
    held.output = output.to_path_buf();
    held.prepared = None;
    held.measured = None;
    held.verified = None;
    held.acquisition_ns = 0;
    held.preparation_added_ns = 0;
    held.operation_ns = 0;
    held.timing_json_bytes = 0;
    held.cpu_at_prepared = None;
    held.cpu_at_measured = None;
    held.process_peak_rss_bytes = 0;
    MEASURED_OPEN.store(false, Ordering::Relaxed);
    HANDOFF_NS.store(0, Ordering::Relaxed);
}

/// Where this invocation writes its artifacts.
pub fn output() -> PathBuf {
    clock().output.clone()
}

/// Closes the preparation phase: everything before this mark is setup.
///
/// Preparation ends exactly where the measured region opens, so this is the mark
/// that arms the handoff accounting rather than a separate flag a driver would have
/// to remember to set.
pub fn prepared() {
    mark_prepared();
    MEASURED_OPEN.store(true, Ordering::Relaxed);
}

/// Declares the whole invocation to be preparation.
///
/// The `prepare` phase of a phase-split row measures nothing: its entire wall is
/// acquisition. It does not arm the handoff accounting, because nothing it does is
/// inside a measured region.
pub fn preparation_is_the_invocation() {
    mark_prepared();
}

fn mark_prepared() {
    let now = Instant::now();
    // Outside the measured region by construction: `ops::measure` marks preparation
    // closed *before* it opens the product's timer.
    let cpu = crate::support::instruments::cpu_now();
    let mut held = clock();
    if held.prepared.is_none() {
        held.prepared = Some(now);
        held.cpu_at_prepared = cpu;
    }
}

/// Closes the measured phase and records the product's own root elapsed time.
///
/// The two are the same boundary observed twice: the mark is the harness's clock,
/// the number is the product's, and a row whose root is missing or clipped is
/// caught by `g7.tree-complete` rather than by this module.
pub fn measured(operation_ns: u64, timing_json_bytes: u64) {
    MEASURED_OPEN.store(false, Ordering::Relaxed);
    let now = Instant::now();
    // Also outside the region: the product's timer has already closed.
    let cpu = crate::support::instruments::cpu_now();
    let rss = crate::support::instruments::lifetime_peak_rss_bytes().unwrap_or(0);
    let mut held = clock();
    if held.measured.is_none() {
        held.cpu_at_measured = cpu;
    }
    // **Every** measured region moves the mark, and the published operation time
    // **accumulates** (owner ruling 2, erratum E5). A row whose measured region is
    // one `Timing::record` sees exactly the previous behaviour — one call sets the
    // mark once and adds its nanoseconds to a zero — so no 217-row number moves. A
    // row whose measured region is N named children gets the **sum** of those
    // children, and a verification span that begins at the *last* child's close
    // rather than at the first's. The earlier form kept the first mark and
    // overwrote the operation time, so for such a row `operation_ns` was the last
    // child alone and `verification_ns` silently swallowed children 2..N.
    held.measured = Some(now);
    held.process_peak_rss_bytes = held.process_peak_rss_bytes.max(rss);
    held.operation_ns = held.operation_ns.saturating_add(operation_ns);
    held.timing_json_bytes = timing_json_bytes;
}

/// Closes the verification phase.
pub fn verified() {
    let now = Instant::now();
    let mut held = clock();
    if held.verified.is_none() {
        held.verified = Some(now);
    }
}

/// Adds one per-sample acquisition span (the copy and the de-warm).
pub fn add_acquisition(nanoseconds: u64) {
    clock().acquisition_ns += nanoseconds;
}

/// Attributes a span of harness **input assembly** to the preparation phase.
///
/// A driver whose input assembly happens *between* its measured children has real
/// invocation time that no declared phase accounts for, and `runner.py`'s
/// reconciliation fails closed on exactly that (`shared/phases.py`
/// `reconcile_invocation`: the wall must be covered by the declared phases within
/// its tolerance). `history.*` reads its corpus between the children, so it
/// declares those spans here, and the four phases then account for the invocation
/// instead of leaving a sixth of it unexplained.
///
/// The span is outside every measured child by construction, so it cannot be
/// double-counted, and the same driver publishes it in its own right
/// (`history.corpus_read_ns`) so a reader can subtract it rather than trust the
/// attribution. This is a declaration of measured time, not a relaxation: it makes
/// the budgeted complete command **larger**, never smaller.
pub fn add_preparation(nanoseconds: u64) {
    let mut held = clock();
    held.preparation_added_ns = held.preparation_added_ns.saturating_add(nanoseconds);
}

/// Runs one piece of harness work, accounting it when the measured region is open.
///
/// This is the per-object handoff: the harness has to hand the product an owned
/// `FinalizedObject` (`Store::accept` takes it by value), so a copy of the canonical
/// bytes happens inside the timed closure. It is harness work, not product work, and
/// absorbing it into the golden number would misstate the product's cost by the
/// amount the harness spent. It is measured and published as `handoff_ns` instead.
pub fn handoff<T>(body: impl FnOnce() -> T) -> T {
    if !MEASURED_OPEN.load(Ordering::Relaxed) {
        return body();
    }
    let started = Instant::now();
    let value = body();
    HANDOFF_NS.fetch_add(started.elapsed().as_nanos() as u64, Ordering::Relaxed);
    value
}

/// Bytes of `timing.json` this invocation wrote.
pub fn timing_json_bytes() -> u64 {
    clock().timing_json_bytes
}

/// The spans this invocation observed, snapshot at `now`.
///
/// `cleanup_ns` is measured rather than inferred: it is the span from the last
/// closed phase to this snapshot, which is where the child destroys its per-case
/// copy and closes. A caller that snapshots early would report a cleanup it did not
/// perform, so the snapshot is taken after teardown and before the trace flush.
pub fn snapshot() -> Phases {
    let now = Instant::now();
    let held = clock();
    // A phase span runs **from the start to its own mark**, not from the mark
    // onwards: the first version of this snapshot measured "time since the mark"
    // and therefore reported the preparation phase as the interval after it ended,
    // which made the four spans overlap and sum past the invocation.
    let since_start = |mark: Option<Instant>| {
        mark.map(|mark| mark.saturating_duration_since(held.start).as_nanos() as u64)
            .unwrap_or(0)
    };
    // The verification span is measured from the close of the measured region to
    // the close of verification; an invocation with no measured region (the
    // deferred `verify` phase) measures it from its own start, because there the
    // whole invocation is verification.
    let verification_ns = match (held.measured, held.verified) {
        (Some(measured), Some(verified)) => {
            verified.saturating_duration_since(measured).as_nanos() as u64
        }
        (None, Some(verified)) => verified.saturating_duration_since(held.start).as_nanos() as u64,
        _ => 0,
    };
    let cleanup_ns = held
        .verified
        .or(held.measured)
        .map(|mark| now.saturating_duration_since(mark).as_nanos() as u64)
        .unwrap_or(0);
    // The measured region's own CPU: the difference between the two boundary reads,
    // saturating because a zero reading means the platform could not produce one.
    let cpu_delta = match (held.cpu_at_prepared, held.cpu_at_measured) {
        (Some(before), Some(after)) => Some((
            after.user_ns.saturating_sub(before.user_ns),
            after.system_ns.saturating_sub(before.system_ns),
        )),
        _ => None,
    };
    Phases {
        preparation_ns: since_start(held.prepared).saturating_add(held.preparation_added_ns),
        acquisition_ns: held.acquisition_ns,
        operation_ns: held.operation_ns,
        verification_ns,
        cleanup_ns,
        handoff_ns: HANDOFF_NS.load(Ordering::Relaxed),
        invocation_ns: now.saturating_duration_since(held.start).as_nanos() as u64,
        timing_json_bytes: held.timing_json_bytes,
        cpu_user_ns: cpu_delta.map(|(user, _)| user).unwrap_or(0),
        cpu_system_ns: cpu_delta.map(|(_, system)| system).unwrap_or(0),
        process_peak_rss_bytes: held.process_peak_rss_bytes,
    }
}

/// Renders the spans as the flat JSON object `phases-<phase>.json` holds.
///
/// Hand-rendered rather than `serde`-derived: the harness's other evidence files
/// are JSON written by hand, the shape is eight scalars, and a dependency added for
/// eight scalars is a dependency the lock-parity test then has to justify.
pub fn render(invocation: &str, phases: &Phases) -> String {
    let mut out = String::from("{\n");
    out.push_str(&format!("  \"schema\": \"layerfs-phases-v1\",\n"));
    out.push_str(&format!("  \"invocation\": \"{invocation}\",\n"));
    let fields = phases.as_fields();
    for (index, (key, value)) in fields.iter().enumerate() {
        let comma = if index + 1 == fields.len() { "" } else { "," };
        out.push_str(&format!("  \"{key}\": {value}{comma}\n"));
    }
    out.push_str("}\n");
    out
}

/// Parses one `phases-<phase>.json` back, so the runner's re-derivation and the
/// child's publication cannot drift into two formats.
pub fn parse(text: &str) -> Result<(String, Phases), String> {
    let mut invocation = String::new();
    let mut phases = Phases::default();
    for line in text.lines() {
        let line = line.trim().trim_end_matches(',');
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let key = key.trim().trim_matches('"');
        let value = value.trim().trim_matches('"');
        if key == "invocation" {
            invocation = value.to_string();
            continue;
        }
        let parsed: u64 = match value.parse() {
            Ok(parsed) => parsed,
            Err(_) => continue,
        };
        match key {
            "preparation_ns" => phases.preparation_ns = parsed,
            "acquisition_ns" => phases.acquisition_ns = parsed,
            "operation_ns" => phases.operation_ns = parsed,
            "verification_ns" => phases.verification_ns = parsed,
            "cleanup_ns" => phases.cleanup_ns = parsed,
            "handoff_ns" => phases.handoff_ns = parsed,
            "invocation_ns" => phases.invocation_ns = parsed,
            "timing_json_bytes" => phases.timing_json_bytes = parsed,
            "cpu_user_ns" => phases.cpu_user_ns = parsed,
            "cpu_system_ns" => phases.cpu_system_ns = parsed,
            "process_peak_rss_bytes" => phases.process_peak_rss_bytes = parsed,
            _ => {}
        }
    }
    if invocation.is_empty() {
        return Err("phases file carries no invocation tag".to_string());
    }
    Ok((invocation, phases))
}
