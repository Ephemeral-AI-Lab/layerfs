//! Real pooled-metadata lane: save-to-acknowledgement and readback, one sample.
//!
//! This is a wiring, correctness and single-sample wiring demonstration for the
//! physical metadata encoding, not a benchmark: one sample of one deterministic
//! fixture, a fresh output directory per run, the real product result, and the
//! counters the real operation reported. Timing values are exploratory and are not
//! a release qualification.
//!
//! Usage:
//!   measure_pooled --leaves 128 --rows 100 --output FRESH_DIR [--timing on|off]
//!
//! Cache state: the fixture is built by this process immediately before the timed
//! scope, and the Store is created inside the run, so no warm-cache credit is
//! claimed for any phase.
//!
//! `--timing off` runs the same bodies through the timing-disabled path, so the
//! identities, counters and retained footprint of the two modes can be compared.

use std::fs::OpenOptions;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

use layerfs_content::inode_leaf::{
    encode_inode_value, InodeKind, InodeLeaf, InodeLeafRow, InodeValue, INODE_VALUE_BYTES,
};
use layerfs_content::{
    AdvisoryPredecessors, FinalizedObject, ObjectId, ObjectRole, PredecessorProvenance,
};
use layerfs_storage::{StorageError, StoragePolicy, Store};
use layerfs_telemetry::timer::{Timing, TimingReport, TimingScope};

type Failure = Box<dyn std::error::Error>;

/// Result of the whole save-to-acknowledgement phase.
type SavePhase = (Store, Totals, Vec<ObjectId>);
/// Result of the readback phase: the canonical bytes and objects returned.
type ReadPhase = (Vec<Vec<u8>>, u64);

/// Largest number of leaves this demonstration stores.
const LEAF_LIMIT: u64 = 4_096;
/// Largest number of rows one leaf may carry.
const ROW_LIMIT: u64 = 200;

struct Options {
    leaves: u64,
    rows: u64,
    output: PathBuf,
    timing: bool,
}

fn parse_options() -> Result<Options, Failure> {
    let mut leaves = 128_u64;
    let mut rows = 100_u64;
    let mut output = None;
    let mut timing = true;
    let mut arguments = std::env::args().skip(1);
    while let Some(flag) = arguments.next() {
        let value = arguments
            .next()
            .ok_or_else(|| format!("{flag} needs a value"))?;
        match flag.as_str() {
            "--leaves" => leaves = value.parse().map_err(|_| "invalid leaves")?,
            "--rows" => rows = value.parse().map_err(|_| "invalid rows")?,
            "--output" => output = Some(PathBuf::from(value)),
            "--timing" => {
                timing = match value.as_str() {
                    "on" => true,
                    "off" => false,
                    other => return Err(format!("unsupported timing mode {other}").into()),
                }
            }
            other => return Err(format!("unsupported argument {other}").into()),
        }
    }
    let output = output.ok_or("--output is required")?;
    if output.exists() {
        return Err(format!("--output {} already exists", output.display()).into());
    }
    if !(1..=LEAF_LIMIT).contains(&leaves) {
        return Err(format!("--leaves must be 1..={LEAF_LIMIT}").into());
    }
    if !(2..=ROW_LIMIT).contains(&rows) {
        return Err(format!("--rows must be 2..={ROW_LIMIT}").into());
    }
    std::fs::create_dir_all(&output)?;
    Ok(Options {
        leaves,
        rows,
        output,
        timing,
    })
}

fn value(seed: u64) -> [u8; INODE_VALUE_BYTES] {
    let mut bytes = [0_u8; 32];
    bytes[..8].copy_from_slice(&seed.to_be_bytes());
    encode_inode_value(InodeValue {
        kind: InodeKind::RegularFile,
        namespace_ref_count: 1,
        content_root: ObjectId::for_bytes(&bytes),
        metadata_root: ObjectId::for_bytes(&[seed as u8; 8]),
    })
}

/// One canonical leaf that shares every row but its last with the leaf before it,
/// so the pooled lane can store it as a COPY/INSERT delta when one is admitted.
fn leaf(rows: u64, last_seed: u64) -> (FinalizedObject, Vec<u8>) {
    let mut values = (0..rows - 1).map(value).collect::<Vec<_>>();
    values.push(value(last_seed));
    let rows = values
        .iter()
        .enumerate()
        .map(|(index, value)| InodeLeafRow {
            serial: 1 + index as u64,
            value: *value,
        })
        .collect::<Vec<_>>();
    let canonical = InodeLeaf {
        subtree_bytes: rows.len() as u64 * INODE_VALUE_BYTES as u64,
        rows,
    }
    .encode()
    .expect("canonical leaf");
    let object =
        FinalizedObject::new(ObjectRole::InodeLeaf, canonical.clone()).expect("finalized leaf");
    (object, canonical)
}

fn with_predecessor(object: FinalizedObject, base: ObjectId) -> Result<FinalizedObject, Failure> {
    let mut predecessors = AdvisoryPredecessors::new();
    predecessors.push(base, PredecessorProvenance::OriginalBase)?;
    Ok(object.with_predecessors(predecessors))
}

/// Aggregate counters of the whole save phase.
#[derive(Default, Clone, Copy)]
struct Totals {
    inserted: u64,
    reused: u64,
    packs: u64,
    commits: u64,
    leaves: u64,
    new_values: u64,
    reused_values: u64,
    groups: u64,
    delta_leaves: u64,
    full_leaves: u64,
    trials: u64,
    work_exceeded: u64,
}

/// Runs `operation` with the clocks on or off as requested.
fn timed<T, E, F>(
    options: &Options,
    name: &'static str,
    operation: F,
) -> (Result<T, E>, TimingReport)
where
    F: FnOnce(&TimingScope<'_, layerfs_telemetry::timer::Active>) -> Result<T, E>,
{
    if options.timing {
        Timing::record(name, operation)
    } else {
        Timing::disabled(name, operation)
    }
}

fn render(report: &TimingReport) {
    println!("--- timing tree (inclusive; overlapping spans add to nothing) ---");
    if !report.has_root() {
        println!("disabled: no timing recorded");
        return;
    }
    if let Err(error) = report.write_text(std::io::stdout()) {
        println!("timing render failed: {error}");
    }
}

fn save_report(report: &TimingReport, path: &Path) -> Result<(), Failure> {
    if !report.has_root() {
        return Ok(());
    }
    let file = OpenOptions::new().write(true).create_new(true).open(path)?;
    let mut writer = BufWriter::new(file);
    report.write_json(&mut writer)?;
    writer.flush()?;
    println!(
        "timings: {} nodes at {} levels -> {}",
        report.node_count(),
        report.levels(),
        path.display()
    );
    if report.is_incomplete() {
        // D1: a clipped tree is a hard failure for a measured row, not a note; the
        // file stays on disk because receipts are never withdrawn.
        return Err(format!(
            "timings: INCOMPLETE - the node budget clipped this tree; the run is not a \
             measured row and {} must not be quoted",
            path.display()
        )
        .into());
    }
    Ok(())
}

fn main() -> Result<(), Failure> {
    // D2: the whole-command wall time is printed by the tool itself.
    let started = std::time::Instant::now();
    let result = run();
    println!("wall_seconds: {:.6}", started.elapsed().as_secs_f64());
    result
}

fn run() -> Result<(), Failure> {
    let options = parse_options()?;
    let store_path = options.output.join("store.sqlite");
    let policy = StoragePolicy::new(1, 131_072, 8, 4).validated()?;
    println!("leaves: {}", options.leaves);
    println!("rows-per-leaf: {}", options.rows);
    println!("timing: {}", if options.timing { "on" } else { "off" });
    println!(
        "policy: cutoff {} depths whole-file {} chunk {} metadata {}",
        policy.small_file_threshold_bytes(),
        policy.whole_file_delta_max_depth(),
        policy.chunk_delta_max_depth(),
        policy.metadata_delta_max_depth()
    );
    println!("store: {}", store_path.display());

    // Save-to-acknowledgement: the Store is created inside the timed scope, and one
    // operation per leaf, because a pooled base must already be stored to be read
    // for a trial.
    let (result, report): (Result<SavePhase, StorageError>, TimingReport) =
        timed(&options, "pooled.save", |scope| {
            let store = Store::create(&store_path, policy, scope.child("store.create"))?;
            let mut totals = Totals::default();
            let mut identities = Vec::new();
            let mut previous = None;
            for step in 0..options.leaves {
                let (object, canonical) = leaf(options.rows, 10_000 + step);
                let object = match previous {
                    Some(base) => with_predecessor(object, base).map_err(|error| {
                        StorageError::Integrity(Box::leak(error.to_string().into_boxed_str()))
                    })?,
                    None => object,
                };
                let id = object.id();
                if ObjectId::for_bytes(&canonical) != id {
                    return Err(StorageError::Integrity("fixture identity"));
                }
                let mut operation = store.begin_save(scope.child("storage.begin"))?;
                operation.accept(object)?;
                let outcome = operation.finish(scope.child("storage.finish"))?;
                totals.inserted += outcome.inserted;
                totals.reused += outcome.reused;
                totals.packs += outcome.packs_created;
                totals.commits += outcome.commits;
                totals.leaves += outcome.pool.leaves;
                totals.new_values += outcome.pool.new_values;
                totals.reused_values += outcome.pool.reused_values;
                totals.groups += outcome.pool.groups;
                totals.delta_leaves += outcome.pool.delta_leaves;
                totals.full_leaves += outcome.pool.full_leaves;
                totals.trials += outcome.pool.trials;
                totals.work_exceeded += outcome.pool.work_exceeded;
                identities.push(id);
                previous = Some(id);
            }
            Ok((store, totals, identities))
        });
    let (store, totals, identities) = result?;
    println!(
        "save-to-ack: inserted {} reused {} packs {} commits {}",
        totals.inserted, totals.reused, totals.packs, totals.commits
    );
    println!(
        "pooled lane: leaves {} new values {} reused values {} groups {} full {} delta {} trials {} work-exceeded {}",
        totals.leaves,
        totals.new_values,
        totals.reused_values,
        totals.groups,
        totals.full_leaves,
        totals.delta_leaves,
        totals.trials,
        totals.work_exceeded
    );

    // Readback of the first and the deepest leaf: byte-exact against the bytes the
    // run admitted, so the pooled reconstruction is verified, not just timed.
    let first = identities[0];
    let last = *identities.last().expect("at least one leaf");
    let (expected_first, _) = leaf(options.rows, 10_000);
    let (expected_last, _) = leaf(options.rows, 10_000 + options.leaves - 1);
    let expected_first = expected_first.canonical().to_vec();
    let expected_last = expected_last.canonical().to_vec();
    let (result, readback): (Result<ReadPhase, StorageError>, TimingReport) =
        timed(&options, "pooled.readback", |scope| {
            let (values, counters) =
                store.read_batch(&[first, last], scope.child("storage.read"))?;
            Ok((values, counters.objects))
        });
    let (values, objects) = result?;
    if values.len() != 2 || values[0] != expected_first || values[1] != expected_last {
        return Err("pooled readback differs from the admitted leaves".into());
    }
    println!("readback: {objects} object(s) verified byte-for-byte");
    // The identities are printed so the observability-equivalence arm can compare
    // the two timing modes on identities, not only on counters.
    println!("readback first: {first}");
    println!("readback last: {last}");

    // Retained footprint of the bounded derivation the Store owns, plus the physical
    // size of what it published.
    let file_bytes = std::fs::metadata(&store_path)?.len();
    let groups = catalogue(&store_path)?;
    println!(
        "retained: pool index {} entries / {} bytes",
        store.pool_index_entries(),
        store.pool_index_bytes()
    );
    println!("store-file: {file_bytes} bytes");
    println!("catalogue: {} group(s), {} ordinal(s)", groups.0, groups.1);
    println!("exclusions: fixture construction and fixture identity checks are inside pooled.save");
    render(&report);
    render(&readback);
    save_report(&report, &options.output.join("pooled-save.json"))?;
    save_report(&readback, &options.output.join("pooled-readback.json"))
}

/// External catalogue inspection: group count and total pooled ordinals.
fn catalogue(path: &Path) -> Result<(u64, u64), Failure> {
    let connection = rusqlite::Connection::open(path)?;
    let mut statement = connection
        .prepare("SELECT count(*), coalesce(sum(count), 0) FROM metadata_value_groups")?;
    let row: (i64, i64) = statement.query_row([], |row| Ok((row.get(0)?, row.get(1)?)))?;
    Ok((row.0 as u64, row.1 as u64))
}
