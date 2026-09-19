//! The shape drivers — the only place operation bodies live.
//!
//! Ten shapes cover the registry; every family calls one of them, so a family
//! module stays thin and a fix to a shape fixes every family that uses it.
//!
//! **The one measurement rule every driver obeys.** A timed phase runs with a
//! *non-retaining* consumer (`DiscardingConsumer`), so nothing the harness does
//! inside the heap window can be attributed to the product, and every oracle is a
//! **second, unmeasured, byte-identical operation** into a `TreeStore`. The two
//! roots must match, which is itself an O1 gate: if the replay did not reproduce
//! the measured operation, the oracle is not describing it. A driver that instead
//! read its oracle out of the measured run's own retained output would make the
//! oracle a function of the operation, and the counting allocator would charge the
//! harness's bookkeeping to the product.
//!
//! The split into `c1`, `c2` and `fs` is a split of *files*, not of rules: the
//! bodies all live under `ops/`.

pub mod c1;
pub mod c2;
pub mod fs;
pub mod fs_fixture;
pub mod pipeline;

use std::path::{Path, PathBuf};

use layerfs_telemetry::timer::{Active, Timing, TimingReport, TimingScope};

use crate::gates::{aggregate, Gate, Status};
use crate::registry::{Case, Shape};
use crate::support::trace::TraceWriter;

/// Which of a row's three phases this invocation runs.
///
/// `benchmark_rules.md` section 6 requires setup, performance and verification to
/// use separate timing and resource scopes, and owner decision D2 already asks for
/// fixture construction to leave the performance invocation. A row that declares
/// `PhaseSplit` runs one invocation per phase against one prepared artifact; every
/// other row runs `Perf` alone and is unchanged.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Phase {
    /// Acquires the row's prepared artifact and does not measure.
    Prepare,
    /// The measured phase. Never builds a fixture and never runs an oracle.
    Perf,
    /// The unmeasured oracle, charged to the verification budget.
    Verify,
}

/// A declared wait state a calibration arm can ask the child to enter.
///
/// This is **harness argv**, not a product surface: it touches no hook, no feature
/// flag and no fault-injection path in `core/crates/*/src`. It exists so the
/// declared-process-kill arm has a point to kill at - a child that has acquired
/// write ownership of a Store and is provably inside a save - which is the one
/// thing `CONTRACT.md` section 7 asks for and nothing in the product provides.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Hold {
    /// File written once the child is inside the save, so the killer can wait for
    /// a real hold point rather than for a wall-clock guess.
    pub marker: std::path::PathBuf,
    /// Nanoseconds to hold before the child continues on its own.
    pub nanos: u64,
    /// Which declared point to hold at.
    pub point: HoldPoint,
}

/// The declared points a calibration arm can kill a child at.
///
/// The two are different states, and the product's own contract distinguishes
/// them: `begin_save` after a killed run is `UninspectedState` only when the
/// killed save had **written packs whose watermark transaction did not commit**.
/// A child killed while merely holding ownership has written nothing, so the next
/// `begin_save` is correctly accepted; an arm that kills there measures the wrong
/// state and would report the product as wrong for being right.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HoldPoint {
    /// After `begin_save` acquired ownership and before anything is accepted.
    Begin,
    /// After the offered objects are accepted and before `finish` commits the
    /// publication watermark.
    Accept,
}

/// Where one case writes, and what it was handed.
pub struct OpContext<'a> {
    /// Fresh output directory for this case. It must not already exist.
    pub output: &'a Path,
    /// Store path for a C2 row, when the runner supplied one.
    pub store: Option<PathBuf>,
    /// Object directory for a row that opens a prepared artifact.
    pub objects: Option<PathBuf>,
    /// Directory for the prepared filesystem input, when the runner cached one.
    pub prepared_input: Option<PathBuf>,
    /// `true` to load `prepared_input` instead of building and emitting it.
    pub load_input: bool,
    /// Which phase this invocation runs.
    pub phase: Phase,
    /// Declared wait state, when a calibration arm asked for one.
    pub hold: Option<Hold>,
    /// `--verify-sample N`: verify a deterministic sample of `N` units, or declare
    /// the unit count and take the declared 10% when `N` is zero.
    ///
    /// This is harness argv, like `--hold-save`: it selects how much of the oracle
    /// runs, and a row that sampled is `INCOMPLETE` in the receipt, never `PASS`.
    pub verify_sample: Option<usize>,
    /// Trace writer for this case.
    pub trace: &'a mut TraceWriter,
}

/// The declared sampling rule: `max(1, ceil(units/10))` units, selected by
/// `index % 10 == 0` in declaration order.
///
/// One rule, stated once and applied wherever a row declares a countable
/// verification unit. It is never "the first ten": a prefix sample and a strided
/// sample are different claims, and the strided one is the one that spreads across
/// the whole input.
pub fn sampled_indices(units: usize, requested: usize) -> Vec<usize> {
    let selected = if requested == 0 {
        units.div_ceil(10).max(1)
    } else {
        requested.min(units.max(1))
    };
    (0..units).filter(|index| index % 10 == 0).take(selected).collect()
}

impl OpContext<'_> {
    /// Enters the declared wait state, when one was asked for.
    ///
    /// Returns whether it held. The marker is written **before** the sleep, so a
    /// killer waits for the child's own declaration that it reached the point
    /// rather than for a wall-clock guess.
    pub fn hold_at(&self, point: HoldPoint) -> Result<bool, OpError> {
        let Some(hold) = &self.hold else {
            return Ok(false);
        };
        if hold.point != point {
            return Ok(false);
        }
        std::fs::write(&hold.marker, "held")
            .map_err(|error| OpError::Io(format!("{}: {error}", hold.marker.display())))?;
        std::thread::sleep(std::time::Duration::from_nanos(hold.nanos));
        Ok(true)
    }

    /// The artifact directory a phase-split row's phases share.
    ///
    /// A row whose registry declaration says it needs a master is handed one by
    /// `runner.py prepare` through `--emit-input` (acquisition) or `--load-input`
    /// (every later phase). A row that declares a master and is handed none is a
    /// driver defect and fails closed with that reason rather than rebuilding the
    /// fixture inside the invocation the phases exist to keep it out of.
    pub fn artifact_directory(&self) -> Result<PathBuf, OpError> {
        self.prepared_input.clone().ok_or_else(|| {
            OpError::Io(
                "this row declares a prepared master and needs --emit-input/--load-input".to_string(),
            )
        })
    }

    /// Ensures the case's output directory exists.
    ///
    /// The append-only refusal lives in `main`, which owns the decision about
    /// whether this case has already been run: a driver that re-refused an
    /// existing directory would make every second-order write inside its own
    /// output path fail.
    pub fn create_output(&self) -> Result<(), OpError> {
        std::fs::create_dir_all(self.output)
            .map_err(|error| OpError::Io(format!("{}: {error}", self.output.display())))
    }
}

/// What one driver produced.
pub struct OpOutcome {
    /// Aggregate row status: the worst gate wins.
    pub gates: Vec<Gate>,
    /// Free-form declarations the receipt must carry.
    pub notes: Vec<String>,
}

impl OpOutcome {
    /// Empty outcome.
    pub fn new() -> Self {
        Self {
            gates: Vec::new(),
            notes: Vec::new(),
        }
    }

    /// Adds a gate.
    pub fn gate(mut self, gate: Gate) -> Self {
        self.gates.push(gate);
        self
    }

    /// Adds several gates.
    pub fn gates(mut self, gates: impl IntoIterator<Item = Gate>) -> Self {
        self.gates.extend(gates);
        self
    }

    /// Records a declaration.
    pub fn note(mut self, note: impl Into<String>) -> Self {
        self.notes.push(note.into());
        self
    }

    /// The aggregate row status.
    pub fn status(&self) -> Status {
        aggregate(&self.gates)
    }
}

impl Default for OpOutcome {
    fn default() -> Self {
        Self::new()
    }
}

/// Why a driver could not produce a row.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OpError {
    /// The shape has no driver yet. The row is `NOT_RUN`, never a `PASS`.
    Unimplemented(&'static str),
    /// The product returned an error. The row reports it verbatim.
    Product(String),
    /// The harness could not complete its own work.
    Io(String),
}

impl From<std::io::Error> for OpError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error.to_string())
    }
}

impl std::fmt::Display for OpError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unimplemented(shape) => write!(formatter, "driver unimplemented: {shape}"),
            Self::Product(message) => write!(formatter, "product error: {message}"),
            Self::Io(message) => write!(formatter, "io error: {message}"),
        }
    }
}

/// Runs a row's measured region, closing the preparation phase before it opens.
///
/// Every driver's measured region is a `Timing::record` closure, so this is the one
/// choke point where the boundary between preparation and the measured operation is
/// known rather than declared: everything before it is setup, everything inside it
/// is the operation the row claims, and everything after it is the oracle. It also
/// writes the product's own timing tree as `timing.json` for **every** row that
/// measured something — `ops/fs.rs` never called `write_timing`, so 56 of the 217
/// passing rows published no operation time at all.
pub fn measure<T, E, F>(
    name: impl Into<std::borrow::Cow<'static, str>>,
    operation: F,
) -> (Result<T, E>, TimingReport)
where
    F: FnOnce(&TimingScope<'_, Active>) -> Result<T, E>,
{
    crate::support::phases::prepared();
    let (result, report) = Timing::record(name, operation);
    let operation_ns = report
        .root()
        .map(|root| root.elapsed().as_nanos() as u64)
        .unwrap_or(0);
    let directory = crate::support::phases::output();
    let timing_json_bytes = if directory.as_os_str().is_empty() {
        0
    } else {
        c1::write_timing(&directory, &report).unwrap_or(0)
    };
    crate::support::phases::measured(operation_ns, timing_json_bytes);
    (result, report)
}

/// Runs the one case it is handed.
pub fn run(case: &Case, context: &mut OpContext<'_>) -> Result<OpOutcome, OpError> {
    let outcome = match case.shape {
        Shape::Construct(route) => c1::construct(case, route, context),
        Shape::ChunkCount(op) => c1::chunk_count(case, op, context),
        Shape::Edit(op) => c1::edit(case, op, context),
        Shape::Transition(target) => c1::transition(case, target, context),
        Shape::Lifecycle(step) => c2::lifecycle(case, step, context),
        Shape::Reuse(op) => c2::reuse(case, op, context),
        Shape::Delta(op) => c2::delta(case, op, context),
        Shape::Boundary { seed } => c2::boundary(case, seed, context),
        Shape::SmallFile => c2::small_file(case, context),
        Shape::ReadWave => c2::read_wave(case, context),
        Shape::Footprint(op) => c2::footprint(case, op, context),
        Shape::ManyTiny(op) => fs::many_tiny(case, op, context),
        Shape::Tree(op) => fs::tree(case, op, context),
        Shape::Namespace => fs::namespace(case, context),
        Shape::Locality(op) => fs::locality(case, op, context),
        Shape::FsBuild { text } => fs::fs_build(case, text, context),
        Shape::Workspace(op) => c2::workspace(case, op, context),
        Shape::Pool { cold } => c2::pool(case, cold, context),
        Shape::Pipeline(op) => pipeline::run(case, op, context),
        Shape::Primitives(_) => Err(OpError::Unimplemented("component-primitives")),
    };
    // The oracle is the last thing every driver does before it returns its
    // outcome, so this is where verification ends. Gate assembly is a few
    // microseconds of harness bookkeeping and is charged here rather than hidden.
    crate::support::phases::verified();
    outcome
}

/// Stable 64-bit seed for a case, derived from its ID.
///
/// A case's fixture must be the same fixture every time it runs, including after
/// a rebuild, so the seed is a function of the ID and never of the run.
pub fn seed_of(id: &str) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in id.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// The salt that makes a replay operation distinguishable in a seed while still
/// deterministic.
pub const REPLAY_SALT: u64 = 0x5eed_0000_0000_0001;
