//! The PASS/FAIL decision. Pure, and the highest-value test target.
//!
//! Two consequences of `claim_kind = structural-complexity` decide everything
//! here:
//!
//! * every gate is **absolute and single-arm** — no gate compares this tree with
//!   another tree, because no legitimate matched pair exists beyond the three
//!   diagnostic `component.primitives` rows;
//! * **`elapsed_ns` never gate-decides.** The established same-binary wall spread
//!   is +17.6 %, which makes the O(n) *time* band overlap the O(n log n) one and
//!   cannot separate O(n) from O(1). Scaling gates read counters, heap and disk.
//!
//! Nothing in this module reads a clock, a file or a counter. It takes numbers and
//! returns a status, so every band and every rule below is testable without running
//! a benchmark.

/// The frozen status vocabulary.
///
/// The distinctions are load-bearing: an unavailable measurement is `INCOMPLETE`,
/// never a `PASS`, and a failed precondition is `INELIGIBLE` rather than `FAIL`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Status {
    /// Oracle matched and every gate held.
    Pass,
    /// A frozen gate was missed, with valid evidence.
    Fail,
    /// The numerical target was missed but the row is otherwise valid.
    TargetMiss,
    /// A required measurement or counter is unavailable. Never a `PASS`.
    Incomplete,
    /// A precondition failed: residency, coverage, cache arm mismatch.
    Ineligible,
    /// Not attempted, with the reason.
    NotRun,
}

impl Status {
    /// Severity used to aggregate a row.
    ///
    /// `NOT_RUN` outranks `TARGET_MISS` because a row with no evidence is weaker
    /// than a row with valid evidence and a missed number.
    pub const fn severity(self) -> u8 {
        match self {
            Self::Pass => 0,
            Self::TargetMiss => 1,
            Self::NotRun => 2,
            Self::Ineligible => 3,
            Self::Incomplete => 4,
            Self::Fail => 5,
        }
    }

    /// Token as it appears in a receipt and in the report.
    pub const fn token(self) -> &'static str {
        match self {
            Self::Pass => "PASS",
            Self::Fail => "FAIL",
            Self::TargetMiss => "TARGET_MISS",
            Self::Incomplete => "INCOMPLETE",
            Self::Ineligible => "INELIGIBLE",
            Self::NotRun => "NOT_RUN",
        }
    }
}

/// The seven gate classes, frozen in `gates_and_oracles.md` section 3.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GateClass {
    /// G1 correctness: the independent oracle matched.
    Correctness,
    /// G2 mechanism: the declared route was taken, proven by counters.
    Mechanism,
    /// G3 scaling: the per-doubling ratio lies inside the declared band.
    Scaling,
    /// G4 resource: declared ceilings, residency and swap gates held.
    Resource,
    /// G5 cleanup: cleanup ran once and passed, master unchanged, no sidecars.
    Cleanup,
    /// G6 custody: identities pinned, seals matched, receipt append-only.
    Custody,
    /// G7 timing purity: the tree is complete and no verifier work is inside it.
    TimingPurity,
}

impl GateClass {
    /// Token used in receipts.
    pub const fn token(self) -> &'static str {
        match self {
            Self::Correctness => "G1",
            Self::Mechanism => "G2",
            Self::Scaling => "G3",
            Self::Resource => "G4",
            Self::Cleanup => "G5",
            Self::Custody => "G6",
            Self::TimingPurity => "G7",
        }
    }
}

/// One gate outcome, with the numbers that decided it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Gate {
    /// Gate class.
    pub class: GateClass,
    /// Stable identifier of this check.
    pub id: &'static str,
    /// Outcome.
    pub status: Status,
    /// What was measured, with its unit.
    pub measured: String,
    /// What it was measured against.
    pub limit: String,
}

impl Gate {
    /// A gate that held.
    pub fn pass(class: GateClass, id: &'static str, measured: &str, limit: &str) -> Self {
        Self {
            class,
            id,
            status: Status::Pass,
            measured: measured.to_string(),
            limit: limit.to_string(),
        }
    }

    /// A gate that did not hold, with valid evidence.
    pub fn fail(class: GateClass, id: &'static str, measured: &str, limit: &str) -> Self {
        Self {
            class,
            id,
            status: Status::Fail,
            measured: measured.to_string(),
            limit: limit.to_string(),
        }
    }

    /// A gate whose measurement is unavailable.
    pub fn incomplete(class: GateClass, id: &'static str, measured: &str, limit: &str) -> Self {
        Self {
            class,
            id,
            status: Status::Incomplete,
            measured: measured.to_string(),
            limit: limit.to_string(),
        }
    }

    /// A gate whose precondition failed.
    pub fn ineligible(class: GateClass, id: &'static str, measured: &str, limit: &str) -> Self {
        Self {
            class,
            id,
            status: Status::Ineligible,
            measured: measured.to_string(),
            limit: limit.to_string(),
        }
    }

    /// A gate that was not attempted.
    pub fn not_run(class: GateClass, id: &'static str, measured: &str, limit: &str) -> Self {
        Self {
            class,
            id,
            status: Status::NotRun,
            measured: measured.to_string(),
            limit: limit.to_string(),
        }
    }
}

/// Aggregates gates into one row status: the worst gate wins.
pub fn aggregate(gates: &[Gate]) -> Status {
    let mut worst = Status::Pass;
    for gate in gates {
        if gate.status.severity() > worst.severity() {
            worst = gate.status;
        }
    }
    worst
}

/// A boolean gate, expressed as a limit so a failure names what it missed.
pub fn require(class: GateClass, id: &'static str, held: bool, measured: &str, limit: &str) -> Gate {
    if held {
        Gate::pass(class, id, measured, limit)
    } else {
        Gate::fail(class, id, measured, limit)
    }
}

/// The declared growth of a family's metric.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Growth {
    /// Constant in n.
    Constant,
    /// Linear in n.
    Linear,
    /// n log n.
    Linearithmic,
    /// Quadratic in n.
    Quadratic,
}

/// Which axis a scaling number came from.
///
/// `Time` exists so a diagnostic can be labelled as one. It is deliberately still
/// in the enum: `gates_and_oracles.md` publishes a time band, and a band that can
/// never decide is more honest published beside the others than deleted.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Metric {
    /// A work counter reported by the product.
    Counter,
    /// Harness heap, from the counting allocator.
    Heap,
    /// Disk bytes, from the device attestation instrument.
    Disk,
    /// Wall time. Diagnostic under decision D1; never gate-decides.
    Time,
}

impl Metric {
    /// Whether this metric is allowed to produce a `FAIL`.
    pub const fn gate_decides(self) -> bool {
        !matches!(self, Self::Time)
    }

    /// Token used in receipts.
    pub const fn token(self) -> &'static str {
        match self {
            Self::Counter => "counter",
            Self::Heap => "heap",
            Self::Disk => "disk",
            Self::Time => "time",
        }
    }
}

/// A closed or one-sided band.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Band {
    /// Lower bound, when the band is closed.
    pub low: Option<f64>,
    /// Upper bound, when the band is closed.
    pub high: Option<f64>,
}

impl Band {
    /// Whether `value` lies inside.
    pub fn contains(&self, value: f64) -> bool {
        self.low.is_none_or(|low| value >= low) && self.high.is_none_or(|high| value <= high)
    }

    /// Human-readable form for a receipt.
    pub fn describe(&self) -> String {
        match (self.low, self.high) {
            (Some(low), Some(high)) => format!("[{low:.2}, {high:.2}]"),
            (Some(low), None) => format!(">= {low:.2}"),
            (None, Some(high)) => format!("<= {high:.2}"),
            (None, None) => "unbounded".to_string(),
        }
    }
}

/// The frozen band table.
pub fn band(growth: Growth, metric: Metric) -> Band {
    use Growth::{Constant, Linear, Linearithmic, Quadratic};
    use Metric::{Counter, Disk, Heap, Time};
    match (growth, metric) {
        (Constant, Counter) => Band { low: Some(0.90), high: Some(1.11) },
        (Constant, Heap) | (Constant, Disk) => Band { low: Some(0.90), high: Some(1.11) },
        (Constant, Time) => Band { low: Some(0.60), high: Some(1.67) },
        (Linear, Counter) => Band { low: Some(1.90), high: Some(2.10) },
        (Linear, Heap) | (Linear, Disk) => Band { low: Some(1.80), high: Some(2.22) },
        (Linear, Time) => Band { low: Some(1.60), high: Some(2.40) },
        (Linearithmic, Counter) => Band { low: Some(1.95), high: Some(2.55) },
        (Linearithmic, Heap) | (Linearithmic, Disk) => Band { low: Some(1.90), high: Some(2.60) },
        (Linearithmic, Time) => Band { low: Some(1.60), high: Some(2.90) },
        (Quadratic, Counter) => Band { low: Some(3.6), high: None },
        (Quadratic, Heap) | (Quadratic, Disk) => Band { low: Some(3.4), high: None },
        (Quadratic, Time) => Band { low: Some(2.6), high: None },
    }
}

/// Number of doublings between two sizes.
pub fn doublings(from: f64, to: f64) -> Option<f64> {
    if from <= 0.0 || to <= 0.0 || to == from {
        return None;
    }
    Some((to / from).log2())
}

/// `per_doubling_i = ratio_i ** (1 / doublings_i)`.
///
/// The byte ladder `{1, 10, 100, 500}` MiB contains no doublings, so the raw ratio
/// is not a per-doubling figure; normalising it is what the specification names and
/// what makes one band serve every ladder.
pub fn per_doubling(ratio: f64, from: f64, to: f64) -> Option<f64> {
    let doublings = doublings(from, to)?;
    if ratio <= 0.0 {
        return None;
    }
    Some(ratio.powf(1.0 / doublings))
}

/// Least-squares slope of `log2(value)` on `log2(size)`.
///
/// A four-point ladder supports a slope with a residual; a single ratio does not.
/// The slope is the actual complexity claim, and it is reported beside the
/// per-doubling ratio rather than instead of it.
pub fn slope(points: &[(f64, f64)]) -> Option<f64> {
    let usable: Vec<(f64, f64)> = points
        .iter()
        .copied()
        .filter(|(size, value)| *size > 0.0 && *value > 0.0)
        .map(|(size, value)| (size.log2(), value.log2()))
        .collect();
    if usable.len() < 2 {
        return None;
    }
    let count = usable.len() as f64;
    let mean_x = usable.iter().map(|(x, _)| x).sum::<f64>() / count;
    let mean_y = usable.iter().map(|(_, y)| y).sum::<f64>() / count;
    let covariance: f64 = usable
        .iter()
        .map(|(x, y)| (x - mean_x) * (y - mean_y))
        .sum();
    let variance: f64 = usable.iter().map(|(x, _)| (x - mean_x).powi(2)).sum();
    if variance == 0.0 {
        return None;
    }
    Some(covariance / variance)
}

/// The scaling verdict for one ratio.
///
/// `value == None` means the tier is missing, and a missing tier is `INCOMPLETE`,
/// never `PASS`. A ratio outside the band is a **refutation**, reported as one.
pub fn scaling(
    class: GateClass,
    id: &'static str,
    metric: Metric,
    growth: Growth,
    value: Option<f64>,
    basis: &str,
) -> Gate {
    let band = band(growth, metric);
    match value {
        None => Gate::incomplete(
            class,
            id,
            "tier missing",
            &format!("{} band {}", metric.token(), band.describe()),
        ),
        Some(value) if !metric.gate_decides() => Gate {
            class,
            id,
            status: Status::TargetMiss,
            measured: format!("{value:.4} ({basis})"),
            limit: format!(
                "DIAGNOSTIC: {} band {} never gate-decides under decision D1",
                metric.token(),
                band.describe()
            ),
        },
        Some(value) if band.contains(value) => Gate::pass(
            class,
            id,
            &format!("{value:.4} ({basis})"),
            &format!("{} band {}", metric.token(), band.describe()),
        ),
        Some(value) => Gate::fail(
            class,
            id,
            &format!("{value:.4} ({basis})"),
            &format!("{} band {}", metric.token(), band.describe()),
        ),
    }
}

/// Device attestation: a row claiming a cold or de-warmed read must show
/// `disk_read_bytes >= 0.9 x requested`.
///
/// `mincore` alone cannot distinguish a cache-served read from a device read, and
/// that is exactly the #151/L18 error: a transfer that read its own recent writes
/// out of cache measured 19 GB/s against 2.1 GiB/s from storage.
pub fn device_attestation(requested: u64, measured: Option<u64>) -> Gate {
    const GATE: &str = "g4.device-attestation";
    match measured {
        None => Gate::incomplete(
            GateClass::Resource,
            GATE,
            "disk_read_bytes unavailable on this host",
            ">= 0.9 x requested",
        ),
        Some(measured) => {
            let threshold = (requested as f64 * 0.9) as u64;
            if measured >= threshold {
                Gate::pass(
                    GateClass::Resource,
                    GATE,
                    &format!("{measured} bytes read from device"),
                    &format!(">= {threshold} bytes (0.9 x {requested} requested)"),
                )
            } else {
                Gate::ineligible(
                    GateClass::Resource,
                    GATE,
                    &format!("{measured} bytes read from device"),
                    &format!(">= {threshold} bytes (0.9 x {requested} requested)"),
                )
            }
        }
    }
}

/// The residency gate: `resident_pages == 0` wherever the row claims de-warmed.
pub fn residency_gate(resident_pages: Option<u64>) -> Gate {
    const GATE: &str = "g4.residency";
    match resident_pages {
        None => Gate::incomplete(
            GateClass::Resource,
            GATE,
            "residency unreadable",
            "resident_pages == 0",
        ),
        Some(0) => Gate::pass(GateClass::Resource, GATE, "0 resident pages", "== 0"),
        Some(pages) => Gate::ineligible(
            GateClass::Resource,
            GATE,
            &format!("{pages} resident pages"),
            "== 0",
        ),
    }
}

/// The swap gate: `swaps == 0` is a hard failure anything else.
pub fn swap_gate(swaps: Option<u64>) -> Gate {
    const GATE: &str = "g4.swaps";
    match swaps {
        None => Gate::incomplete(GateClass::Resource, GATE, "swaps unreadable", "== 0"),
        Some(0) => Gate::pass(GateClass::Resource, GATE, "0", "== 0"),
        Some(count) => Gate::fail(
            GateClass::Resource,
            GATE,
            &format!("{count}"),
            "== 0 (a non-zero swap count is a hard failure)",
        ),
    }
}

/// Allocation attribution: any row gating `store_allocated_bytes` must be
/// `exclusive`, because a COW clone's `st_blocks` double-counts blocks shared with
/// its master.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Attribution {
    /// The bytes belong to this row alone.
    Exclusive,
    /// The bytes are shared with a master and must not be gated.
    SharedWithMaster,
}

/// The attribution gate.
pub fn attribution_gate(attribution: Attribution) -> Gate {
    const GATE: &str = "g4.allocation-attribution";
    match attribution {
        Attribution::Exclusive => Gate::pass(
            GateClass::Resource,
            GATE,
            "exclusive",
            "exclusive for any row gating allocated bytes",
        ),
        Attribution::SharedWithMaster => Gate::ineligible(
            GateClass::Resource,
            GATE,
            "shared-with-master",
            "exclusive for any row gating allocated bytes; the reflink rung is forbidden here",
        ),
    }
}

/// The `pack_bodies <= database` accounting gate (O6).
///
/// `pack_bodies == 0` with a non-empty store is `INCOMPLETE`, never a pass: the
/// lifted SQL `SELECT COALESCE(SUM(length(data)), 0) FROM object_packs` returns
/// zero after a table rename, and `0 <= database` passes. A missing table is
/// `INCOMPLETE`, and `space.py` asserts the table exists rather than using
/// `COALESCE`.
pub fn pack_accounting(pack_bodies: Option<u64>, database_bytes: u64, objects: u64) -> Gate {
    const GATE: &str = "g1.pack-accounting";
    match pack_bodies {
        None => Gate::incomplete(
            GateClass::Correctness,
            GATE,
            "object_packs table absent or unreadable",
            "pack_bodies <= database",
        ),
        Some(bodies) if bodies == 0 && objects > 0 && database_bytes > 0 => Gate::incomplete(
            GateClass::Correctness,
            GATE,
            &format!("0 pack bytes for {objects} objects"),
            "a non-empty store cannot have zero pack bytes",
        ),
        Some(bodies) if bodies <= database_bytes => Gate::pass(
            GateClass::Correctness,
            GATE,
            &format!("{bodies} <= {database_bytes}"),
            "pack_bodies <= database",
        ),
        Some(bodies) => Gate::fail(
            GateClass::Correctness,
            GATE,
            &format!("{bodies} > {database_bytes}"),
            "pack_bodies <= database",
        ),
    }
}

/// The timing-purity gate: the product's own tree must not be clipped.
///
/// Clipping is silent by design (`MAX_NODES = 1_024`, `MAX_DEPTH = 32`,
/// `MAX_LABEL_BYTES = 128`), and it marks the node **and all its ancestors**
/// incomplete, so the harness converts it to a hard failure rather than letting a
/// clipped tree pass as a measured one.
pub fn timing_purity(is_incomplete: bool, nodes: u64, depth: u64) -> Gate {
    const GATE: &str = "g7.tree-complete";
    if is_incomplete {
        Gate::fail(
            GateClass::TimingPurity,
            GATE,
            &format!("clipped tree: {nodes} nodes, depth {depth}"),
            "report.is_incomplete() == false",
        )
    } else {
        Gate::pass(
            GateClass::TimingPurity,
            GATE,
            &format!("complete tree: {nodes} nodes, depth {depth}"),
            "report.is_incomplete() == false",
        )
    }
}
