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

use crate::gates::{aggregate, Gate, Status};
use crate::registry::{Case, Shape};
use crate::support::trace::TraceWriter;

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
    /// Trace writer for this case.
    pub trace: &'a mut TraceWriter,
}

impl OpContext<'_> {
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

/// Runs the one case it is handed.
pub fn run(case: &Case, context: &mut OpContext<'_>) -> Result<OpOutcome, OpError> {
    match case.shape {
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
    }
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
