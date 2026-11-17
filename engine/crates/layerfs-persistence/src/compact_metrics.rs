//! Bounded metrics and SLO signals for SQLite (§7) and PostgreSQL (§8)
//! Store compaction.
//!
//! Every metric a real `store.compact` or `store.status` path records goes
//! through this module. Label values come from closed enums ([`CompactOutcome`],
//! [`CompactPhase`], [`CompactRefusal`], [`CompactStatusState`]), so the metric
//! cardinality is fixed: no Store name, path, operation id or artifact name is
//! ever a label. A finished run is observed once, in [`record_run`], and
//! recording never changes its result.
//!
//! Production reads the process totals through [`snapshot`]. A test or a
//! diagnostic probe can capture only its own thread with
//! [`CompactMetricsCapture`], so concurrent work on other threads does not
//! pollute the observation.

use crate::backend::sqlite::compact::{CompactKeptWaste, CompactPath, CompactReceipt};
use layerfs_history::error::{HistoryError, HistoryResult};
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

/// The outcome of one Store compaction call.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum CompactOutcome {
    /// A rebuild installed at `G -> G + 1`.
    Rebuild,
    /// The `none` path: mark only, generation unchanged.
    None,
    /// A definite failure that left the live Store unchanged.
    Failed,
    /// A failed or stopped run whose persistence outcome is unknown (§9).
    Uncertain,
}

impl CompactOutcome {
    /// The bounded `outcome` label value.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Rebuild => "rebuild",
            Self::None => "none",
            Self::Failed => "failed",
            Self::Uncertain => "uncertain",
        }
    }
}

/// One phase a run may time.
///
/// SQLite (§7) reports `mark`, `build`, `verify` and `install`. PostgreSQL
/// (§8) reports its own Phase M (`pg_mark`), Phase A (`pg_build`), Phase B
/// (`pg_swap`) and Phase C (`pg_cleanup`). The two backends' values are
/// distinct, so a process total never blends a SQLite phase with a PostgreSQL
/// phase of a different meaning, and every value stays a closed enum string.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum CompactPhase {
    /// The §7 step 1 mark and the wholly-dead-pack decision.
    Mark,
    /// The staged rebuild's build.
    Build,
    /// The immutable verification.
    Verify,
    /// The install and its settle.
    Install,
    /// PostgreSQL §8 Phase M: the intent, the T7.2 mark and the persisted
    /// `gc_dead` set.
    PostgresMark,
    /// PostgreSQL §8 Phase A: copy the retained payload into the scratch
    /// schema.
    PostgresBuild,
    /// PostgreSQL §8 Phase B: swap `pack`/`object_location` at `G -> G + 1`.
    PostgresSwap,
    /// PostgreSQL §8 Phase C: delete the persisted dead set and settle the
    /// totals, also for a resumed `G + 1` leftover.
    PostgresCleanup,
}

impl CompactPhase {
    /// The bounded `phase` label value.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Mark => "mark",
            Self::Build => "build",
            Self::Verify => "verify",
            Self::Install => "install",
            Self::PostgresMark => "pg_mark",
            Self::PostgresBuild => "pg_build",
            Self::PostgresSwap => "pg_swap",
            Self::PostgresCleanup => "pg_cleanup",
        }
    }
}

/// A refusal that stops a compaction before or during its admission.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum CompactRefusal {
    /// A live operation, writer or opener holds the Store.
    Busy,
    /// A dead or uncertain owner left an operation unresolved.
    Unresolved,
}

impl CompactRefusal {
    /// The bounded `code` label value.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Busy => "busy",
            Self::Unresolved => "unresolved",
        }
    }
}

/// The compaction state `store.status` reads from an unsettled op record.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum CompactStatusState {
    /// The owning process still lives.
    Busy,
    /// The recording owner is gone, so the outcome is unresolved.
    Unresolved,
}

impl CompactStatusState {
    /// The bounded `state` label value.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Busy => "busy",
            Self::Unresolved => "unresolved",
        }
    }
}

/// An SLO objective one finished run evaluates.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum CompactSloObjective {
    /// A successful settle left no artifact and zero residue bytes.
    ResidueFree,
    /// A rebuild advanced exactly one generation; `none` kept it.
    Generation,
}

impl CompactSloObjective {
    /// The bounded `objective` label value.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ResidueFree => "residue_free",
            Self::Generation => "generation",
        }
    }
}

/// One SLO check's result.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum CompactSloResult {
    /// The run met the objective.
    Pass,
    /// The run missed the objective.
    Fail,
}

impl CompactSloResult {
    /// The bounded `result` label value.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pass => "pass",
            Self::Fail => "fail",
        }
    }
}

/// A duration aggregate: how many observations, their sum and their maximum,
/// all in nanoseconds.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DurationStat {
    /// Observations counted.
    pub count: u64,
    /// Saturating sum of the observations.
    pub sum_ns: u64,
    /// The longest observation.
    pub max_ns: u64,
}

impl DurationStat {
    fn observe(&mut self, nanos: u64) {
        self.count = self.count.saturating_add(1);
        self.sum_ns = self.sum_ns.saturating_add(nanos);
        self.max_ns = self.max_ns.max(nanos);
    }
}

/// Everything one finished compaction run reports to the metrics.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompactRunObservation {
    /// The run's outcome.
    pub outcome: CompactOutcome,
    /// The generation the mark ran at.
    pub generation_before: u64,
    /// The generation the run left; equal on `none` and on a failure.
    pub generation_after: u64,
    /// §11 `deadBytesBound`.
    pub dead_bytes_bound: u64,
    /// §11 `deadBytesRefined`.
    pub dead_bytes_refined: u64,
    /// §11 `keptWaste`, whose delta-base references are not labels.
    pub kept_waste: CompactKeptWaste,
    /// §11 `residue.bytes`.
    pub residue_bytes: u64,
    /// The distinct live-pack body bytes the Phase M mark read, when the
    /// backend measures it (§11 `markBytes`; a #52 metric, not a report
    /// field). `None` for a backend that does not report the transfer.
    pub mark_bytes: Option<u64>,
    /// The completed phases and their inclusive durations.
    pub phase_nanos: Vec<(CompactPhase, u64)>,
    /// The run's whole duration.
    pub total_nanos: u64,
    /// Whether this run settled an earlier run's leftover record.
    pub leftover_settled: bool,
    /// The admission refusal, when the run was refused.
    pub refusal: Option<CompactRefusal>,
}

/// One rendered metric sample: a stable name, bounded label pairs, a value.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MetricSample {
    /// The metric name, for example `compact.operations`.
    pub name: &'static str,
    /// The bounded label pairs, in sorted key order.
    pub labels: Vec<(&'static str, &'static str)>,
    /// The sample's counter or gauge value.
    pub value: u64,
}

impl MetricSample {
    /// The value of label `key`, when the sample carries it.
    #[must_use]
    pub fn label(&self, key: &str) -> Option<&'static str> {
        self.labels
            .iter()
            .find(|(name, _)| *name == key)
            .map(|(_, value)| *value)
    }
}

/// The last successful run's §11 gauges, the values a report names.
#[derive(Clone, Debug, Eq, PartialEq)]
struct LastRun {
    generation_before: u64,
    generation_after: u64,
    dead_bytes_bound: u64,
    dead_bytes_refined: u64,
    kept_waste: CompactKeptWaste,
    residue_bytes: u64,
    /// §11 `markBytes` when the backend measured it (PostgreSQL); `None` on a
    /// backend whose mark does not report the transfer (`SQLite`).
    mark_bytes: Option<u64>,
}

/// One process's compaction metric totals.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CompactMetrics {
    runs: u64,
    outcomes: BTreeMap<&'static str, u64>,
    refusals: BTreeMap<&'static str, u64>,
    settles: BTreeMap<&'static str, u64>,
    retries: BTreeMap<&'static str, u64>,
    statuses: BTreeMap<&'static str, u64>,
    slo: BTreeMap<(&'static str, &'static str), u64>,
    phases: BTreeMap<&'static str, DurationStat>,
    duration: DurationStat,
    last: Option<LastRun>,
}

impl CompactMetrics {
    /// Adds one finished run.
    fn observe(&mut self, run: &CompactRunObservation) {
        self.runs = self.runs.saturating_add(1);
        increment(&mut self.outcomes, run.outcome.as_str());
        if let Some(refusal) = run.refusal {
            increment(&mut self.refusals, refusal.as_str());
        }
        if run.leftover_settled {
            increment(&mut self.settles, "leftover");
            increment(&mut self.retries, "leftover");
        }
        for (phase, nanos) in &run.phase_nanos {
            self.phases
                .entry(phase.as_str())
                .or_default()
                .observe(*nanos);
        }
        self.duration.observe(run.total_nanos);
        if !matches!(run.outcome, CompactOutcome::Rebuild | CompactOutcome::None) {
            return;
        }
        increment(&mut self.settles, "run");
        let generation = match run.outcome {
            CompactOutcome::Rebuild => {
                run.generation_after == run.generation_before.saturating_add(1)
            }
            CompactOutcome::None => run.generation_after == run.generation_before,
            CompactOutcome::Failed | CompactOutcome::Uncertain => false,
        };
        increment_pair(
            &mut self.slo,
            (
                CompactSloObjective::Generation.as_str(),
                if generation {
                    CompactSloResult::Pass.as_str()
                } else {
                    CompactSloResult::Fail.as_str()
                },
            ),
        );
        increment_pair(
            &mut self.slo,
            (
                CompactSloObjective::ResidueFree.as_str(),
                if run.residue_bytes == 0 {
                    CompactSloResult::Pass.as_str()
                } else {
                    CompactSloResult::Fail.as_str()
                },
            ),
        );
        self.last = Some(LastRun {
            generation_before: run.generation_before,
            generation_after: run.generation_after,
            dead_bytes_bound: run.dead_bytes_bound,
            dead_bytes_refined: run.dead_bytes_refined,
            kept_waste: run.kept_waste.clone(),
            residue_bytes: run.residue_bytes,
            mark_bytes: run.mark_bytes,
        });
    }

    /// Adds one refusal that stopped a call before a run could be observed.
    fn observe_refusal(&mut self, refusal: CompactRefusal) {
        increment(&mut self.refusals, refusal.as_str());
    }

    /// Adds one `store.status` read of an unsettled compaction record.
    fn observe_status(&mut self, state: CompactStatusState) {
        increment(&mut self.statuses, state.as_str());
    }

    /// Every sample this process or capture holds, in stable order.
    ///
    /// Counters are cumulative since the process started (or the capture
    /// started); the gauges are the last successful run's report values.
    #[must_use]
    pub fn samples(&self) -> Vec<MetricSample> {
        let mut samples = Vec::new();
        samples.push(sample("compact.runs", &[], self.runs));
        for (outcome, value) in &self.outcomes {
            samples.push(sample(
                "compact.operations",
                &[("outcome", *outcome)],
                *value,
            ));
        }
        for (code, value) in &self.refusals {
            samples.push(sample("compact.refusals", &[("code", *code)], *value));
        }
        for (kind, value) in &self.settles {
            samples.push(sample("compact.settles", &[("kind", *kind)], *value));
        }
        for (reason, value) in &self.retries {
            samples.push(sample("compact.retries", &[("reason", *reason)], *value));
        }
        for (state, value) in &self.statuses {
            samples.push(sample("compact.status", &[("state", *state)], *value));
        }
        for ((objective, result), value) in &self.slo {
            samples.push(sample(
                "compact.slo",
                &[("objective", *objective), ("result", *result)],
                *value,
            ));
        }
        for (phase, stat) in &self.phases {
            for (statistic, value) in [
                ("count", stat.count),
                ("sum", stat.sum_ns),
                ("max", stat.max_ns),
            ] {
                samples.push(sample(
                    "compact.phase.duration_ns",
                    &[("phase", *phase), ("statistic", statistic)],
                    value,
                ));
            }
        }
        for (statistic, value) in [
            ("count", self.duration.count),
            ("sum", self.duration.sum_ns),
            ("max", self.duration.max_ns),
        ] {
            samples.push(sample(
                "compact.duration_ns",
                &[("statistic", statistic)],
                value,
            ));
        }
        if let Some(last) = &self.last {
            samples.push(sample(
                "compact.dead_bytes",
                &[("kind", "bound")],
                last.dead_bytes_bound,
            ));
            samples.push(sample(
                "compact.dead_bytes",
                &[("kind", "refined")],
                last.dead_bytes_refined,
            ));
            samples.push(sample(
                "compact.kept_waste",
                &[("part", "dead_in_retained_packs")],
                last.kept_waste.dead_in_retained_packs_bytes,
            ));
            samples.push(sample(
                "compact.kept_waste",
                &[("part", "delta_bases")],
                last.kept_waste.delta_bases_bytes,
            ));
            samples.push(sample(
                "compact.kept_waste",
                &[("part", "inode_blocks")],
                last.kept_waste.inode_blocks_bytes,
            ));
            samples.push(sample(
                "compact.kept_waste_bytes",
                &[],
                last.kept_waste
                    .dead_in_retained_packs_bytes
                    .saturating_add(last.kept_waste.delta_bases_bytes)
                    .saturating_add(last.kept_waste.inode_blocks_bytes),
            ));
            samples.push(sample("compact.residue_bytes", &[], last.residue_bytes));
            if let Some(mark_bytes) = last.mark_bytes {
                samples.push(sample("compact.mark_bytes", &[], mark_bytes));
            }
            samples.push(sample(
                "compact.generation",
                &[("which", "before")],
                last.generation_before,
            ));
            samples.push(sample(
                "compact.generation",
                &[("which", "after")],
                last.generation_after,
            ));
        }
        samples
    }
}

fn increment(map: &mut BTreeMap<&'static str, u64>, key: &'static str) {
    let value = map.entry(key).or_insert(0);
    *value = value.saturating_add(1);
}

fn increment_pair(
    map: &mut BTreeMap<(&'static str, &'static str), u64>,
    key: (&'static str, &'static str),
) {
    let value = map.entry(key).or_insert(0);
    *value = value.saturating_add(1);
}

fn sample(name: &'static str, labels: &[(&'static str, &'static str)], value: u64) -> MetricSample {
    MetricSample {
        name,
        labels: labels.to_vec(),
        value,
    }
}

thread_local! {
    /// This thread's armed capture, if any.
    static CAPTURE: RefCell<Option<CompactMetrics>> = const { RefCell::new(None) };
}

fn registry() -> &'static Mutex<CompactMetrics> {
    static METRICS: OnceLock<Mutex<CompactMetrics>> = OnceLock::new();
    METRICS.get_or_init(|| Mutex::new(CompactMetrics::default()))
}

/// Records one finished compaction run in the process totals and, when this
/// thread has an armed [`CompactMetricsCapture`], in that capture too.
pub fn record_run(run: &CompactRunObservation) {
    registry()
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .observe(run);
    CAPTURE.with_borrow_mut(|capture| {
        if let Some(metrics) = capture.as_mut() {
            metrics.observe(run);
        }
    });
}

/// Records one admission refusal that stopped a call before any run.
pub fn record_refusal(refusal: CompactRefusal) {
    registry()
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .observe_refusal(refusal);
    CAPTURE.with_borrow_mut(|capture| {
        if let Some(metrics) = capture.as_mut() {
            metrics.observe_refusal(refusal);
        }
    });
}

/// Records one `store.status` read of an unsettled compaction record.
pub fn record_status(state: CompactStatusState) {
    registry()
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .observe_status(state);
    CAPTURE.with_borrow_mut(|capture| {
        if let Some(metrics) = capture.as_mut() {
            metrics.observe_status(state);
        }
    });
}

/// The process's cumulative compaction metrics.
#[must_use]
pub fn snapshot() -> CompactMetrics {
    registry()
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .clone()
}

/// Clears the process's cumulative compaction metrics.
pub fn reset() {
    *registry()
        .lock()
        .unwrap_or_else(|poison| poison.into_inner()) = CompactMetrics::default();
}

/// Captures the metrics this thread records, leaving the process totals alone.
///
/// Build one with [`Self::start`], run the compaction or status call on the
/// same thread, and read [`Self::finish`]. A capture nests: starting another
/// restores the outer one when it finishes, and a dropped capture restores it
/// too.
pub struct CompactMetricsCapture {
    previous: Option<CompactMetrics>,
    finished: bool,
}

impl CompactMetricsCapture {
    /// Arms a capture on this thread, replacing (and saving) an outer one.
    #[must_use]
    pub fn start() -> Self {
        let previous =
            CAPTURE.with_borrow_mut(|capture| capture.replace(CompactMetrics::default()));
        Self {
            previous,
            finished: false,
        }
    }

    /// The metrics this thread has recorded since the capture started.
    #[must_use]
    pub fn snapshot(&self) -> CompactMetrics {
        CAPTURE.with_borrow(Clone::clone).unwrap_or_default()
    }

    /// Ends the capture, restoring the outer one, and returns its metrics.
    #[must_use]
    pub fn finish(mut self) -> CompactMetrics {
        let metrics = self.snapshot();
        self.finished = true;
        CAPTURE.with_borrow_mut(|capture| *capture = self.previous.take());
        metrics
    }
}

impl Drop for CompactMetricsCapture {
    fn drop(&mut self) {
        if !self.finished {
            CAPTURE.with_borrow_mut(|capture| *capture = self.previous.take());
        }
    }
}

/// The phase clock and §11 values one measured `store.compact` run observes,
/// on either backend. The SQLite §7 phases and the PostgreSQL `pg_` phases are
/// distinct [`CompactPhase`] values, so one clock serves both.
pub(crate) struct RunMetrics {
    pub(crate) started: Instant,
    pub(crate) phase: Option<(CompactPhase, Instant)>,
    pub(crate) phase_nanos: Vec<(CompactPhase, u64)>,
    pub(crate) generation_before: u64,
    pub(crate) generation_after: u64,
    pub(crate) dead_bytes_bound: u64,
    pub(crate) dead_bytes_refined: u64,
    pub(crate) kept_waste: CompactKeptWaste,
    pub(crate) residue_bytes: u64,
    pub(crate) leftover_settled: bool,
    /// §11 `markBytes`, the PostgreSQL Phase M transfer metric. `None` on
    /// SQLite, whose mark reads only metadata bodies and sweeps the payload
    /// in the build copy.
    pub(crate) mark_bytes: Option<u64>,
}

impl RunMetrics {
    pub(crate) fn start(mark_bytes: Option<u64>) -> Self {
        Self {
            started: Instant::now(),
            phase: None,
            phase_nanos: Vec::new(),
            generation_before: 0,
            generation_after: 0,
            dead_bytes_bound: 0,
            dead_bytes_refined: 0,
            kept_waste: CompactKeptWaste::default(),
            residue_bytes: 0,
            leftover_settled: false,
            mark_bytes,
        }
    }

    /// Ends the current phase and starts `phase`; a phase is inclusive wall
    /// time, never an exclusive CPU decomposition.
    pub(crate) fn begin_phase(&mut self, phase: CompactPhase) {
        self.end_phase();
        self.phase = Some((phase, Instant::now()));
    }

    pub(crate) fn end_phase(&mut self) {
        if let Some((phase, started)) = self.phase.take() {
            self.phase_nanos.push((
                phase,
                u64::try_from(started.elapsed().as_nanos()).unwrap_or(u64::MAX),
            ));
        }
    }

    /// Records this run once, whatever its result. The outcome and refusal
    /// come from the typed error, so a `Busy`/`Unresolved` refusal is counted
    /// without changing the error the caller receives.
    pub(crate) fn finish(mut self, result: &HistoryResult<CompactReceipt>) {
        self.end_phase();
        let total_nanos = u64::try_from(self.started.elapsed().as_nanos()).unwrap_or(u64::MAX);
        let (outcome, refusal) = match result {
            Ok(receipt) => {
                // A resumed `G + 1` round reports the original run's
                // generation, so the receipt is the authority here.
                self.generation_before = receipt.generation_before;
                self.generation_after = receipt.generation_after;
                self.dead_bytes_bound = receipt.dead_bytes_bound;
                self.dead_bytes_refined = receipt.dead_bytes_refined;
                self.kept_waste = receipt.kept_waste.clone();
                self.residue_bytes = receipt.residue.bytes;
                let outcome = match receipt.path {
                    CompactPath::Rebuild => CompactOutcome::Rebuild,
                    CompactPath::None => CompactOutcome::None,
                };
                (outcome, None)
            }
            Err(error) => {
                if !error.unknown() {
                    self.generation_after = self.generation_before;
                }
                (
                    if error.unknown() {
                        CompactOutcome::Uncertain
                    } else {
                        CompactOutcome::Failed
                    },
                    match error {
                        HistoryError::Busy | HistoryError::BusyOp(_) => Some(CompactRefusal::Busy),
                        HistoryError::Unresolved(_) => Some(CompactRefusal::Unresolved),
                        _ => None,
                    },
                )
            }
        };
        record_run(&CompactRunObservation {
            outcome,
            generation_before: self.generation_before,
            generation_after: self.generation_after,
            dead_bytes_bound: self.dead_bytes_bound,
            dead_bytes_refined: self.dead_bytes_refined,
            kept_waste: self.kept_waste,
            residue_bytes: self.residue_bytes,
            mark_bytes: self.mark_bytes,
            phase_nanos: self.phase_nanos,
            total_nanos,
            leftover_settled: self.leftover_settled,
            refusal,
        });
    }
}
