//! Real C1-only, C2-only and integrated runs of the two candidate components.
//!
//! This is a wiring demonstration, not a benchmark: it runs one sample of one
//! input per mode with fresh outputs, prints the actual product result and the
//! timing scopes it recorded, and verifies readback wherever it stored.
//!
//! Usage:
//!   measure_components --mode c1 --input PATH --timings FRESH.json
//!   measure_components --mode c2 --input PATH --store FRESH.sqlite --timings FRESH.json
//!   measure_components --mode pipeline --input PATH --store FRESH.sqlite --timings FRESH.json
//!
//! Cache state: each mode reads one explicit small input that the caller has just
//! written, so no warm-cache credit is claimed for any phase. Timing values are
//! exploratory; they are not a release qualification.

use std::fs::OpenOptions;
use std::io::{BufWriter, Read, Write};
use std::path::{Path, PathBuf};

use layerfs_content::{
    construct_bytes, construct_stream, read_all, AuthenticatedObjects, ConstructionPolicy,
    ContentError, ContentResult, DiscardingConsumer, ObjectId,
};
use layerfs_storage::{SaveHandoff, StorageError, StoragePolicy, Store};
use layerfs_telemetry::timer::{Timing, TimingReport};

/// Largest explicit demo input; larger inputs are rejected rather than collected.
const DEMO_INPUT_LIMIT: u64 = 8 * 1024 * 1024;
/// Largest canonical fixture handed to the C2-only mode.
const C2_FIXTURE_BYTES: usize = 131_071;

type Failure = Box<dyn std::error::Error>;

/// Product result of one integrated pipeline run.
type PipelineResult = (
    layerfs_content::ConstructedFile,
    layerfs_storage::SaveOutcome,
    Vec<u8>,
);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Mode {
    C1,
    C2,
    Pipeline,
}

impl Mode {
    fn parse(value: &str) -> Result<Self, Failure> {
        match value {
            "c1" => Ok(Self::C1),
            "c2" => Ok(Self::C2),
            "pipeline" => Ok(Self::Pipeline),
            other => Err(format!("unsupported mode {other}").into()),
        }
    }

    fn uses_storage(self) -> bool {
        !matches!(self, Self::C1)
    }
}

struct Options {
    mode: Mode,
    input: PathBuf,
    timings: PathBuf,
    store: Option<PathBuf>,
}

fn parse_options() -> Result<Options, Failure> {
    let mut mode = None;
    let mut input = None;
    let mut timings = None;
    let mut store = None;
    let mut arguments = std::env::args().skip(1);
    while let Some(flag) = arguments.next() {
        let value = arguments
            .next()
            .ok_or_else(|| format!("{flag} needs a value"))?;
        match flag.as_str() {
            "--mode" => mode = Some(Mode::parse(&value)?),
            "--input" => input = Some(PathBuf::from(value)),
            "--timings" => timings = Some(PathBuf::from(value)),
            "--store" => store = Some(PathBuf::from(value)),
            other => return Err(format!("unsupported argument {other}").into()),
        }
    }
    let mode = mode.ok_or("--mode is required")?;
    let input = input.ok_or("--input is required")?;
    let timings = timings.ok_or("--timings is required")?;
    if mode.uses_storage() && store.is_none() {
        return Err("--store is required for modes that store".into());
    }
    if !mode.uses_storage() && store.is_some() {
        return Err("--store is only accepted when storage is used".into());
    }
    if timings.exists() {
        return Err(format!("--timings {} already exists", timings.display()).into());
    }
    if let Some(path) = store.as_ref() {
        if path.exists() {
            return Err(format!("--store {} already exists", path.display()).into());
        }
    }
    Ok(Options {
        mode,
        input,
        timings,
        store,
    })
}

fn read_input(path: &Path) -> Result<Vec<u8>, Failure> {
    let metadata = std::fs::metadata(path)?;
    if metadata.len() > DEMO_INPUT_LIMIT {
        return Err(format!(
            "demo input is {} bytes; this example refuses inputs above {DEMO_INPUT_LIMIT}",
            metadata.len()
        )
        .into());
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    std::fs::File::open(path)?.read_to_end(&mut bytes)?;
    Ok(bytes)
}

fn save_timings(report: &TimingReport, path: &Path) -> Result<(), Failure> {
    if !report.has_root() {
        println!(
            "timings: not recorded; no file written to {}",
            path.display()
        );
        return Ok(());
    }
    let file = OpenOptions::new().write(true).create_new(true).open(path)?;
    let mut writer = BufWriter::new(file);
    report.write_json(&mut writer)?;
    writer.flush()?;
    println!(
        "timings: {} nodes at {} levels",
        report.node_count(),
        report.levels()
    );
    if report.is_incomplete() {
        println!("timings: INCOMPLETE — detail was clipped, not zero");
    }
    println!("timings: written to {}", path.display());
    Ok(())
}

fn render(report: &TimingReport) {
    println!("--- timing tree (inclusive; overlapping spans add to nothing) ---");
    if !report.has_root() {
        println!("disabled: no timing recorded");
        return;
    }
    match report.write_text(std::io::stdout()) {
        Ok(()) => {}
        Err(error) => println!("timing render failed: {error}"),
    }
}

/// C1 only: stable input, real constructor, bounded non-persisting consumer.
fn run_c1(options: &Options, bytes: &[u8]) -> Result<(), Failure> {
    let policy = ConstructionPolicy::frozen_default();
    let mut consumer = DiscardingConsumer::new();
    let (result, report) = Timing::record("c1.only", |root| {
        construct_stream(
            policy,
            &policy.capacities(),
            bytes,
            &mut consumer,
            root.child("content.construct"),
        )
    });
    let constructed = result.map_err(|error| format!("construction failed: {error}"))?;
    println!("mode: c1");
    println!("root: {}", constructed.root);
    println!("logical length: {}", constructed.logical_len);
    println!("emitted objects: {}", consumer.objects());
    println!("emitted canonical bytes: {}", consumer.canonical_bytes());
    println!("exclusions: no storage, no pack, no database, no input fixture copy");
    render(&report);
    save_timings(&report, &options.timings)
}

/// C2 only: one bounded canonical fixture prepared before the storage scope.
fn run_c2(options: &Options, bytes: &[u8]) -> Result<(), Failure> {
    let fixture_end = bytes.len().min(C2_FIXTURE_BYTES);
    let fixture_raw = bytes[..fixture_end].to_vec();
    println!("mode: c2");
    println!(
        "fixture: one canonical whole-file object built from the first {} bytes of the input",
        fixture_raw.len()
    );
    println!("exclusions: fixture construction is outside the storage scope below");

    let policy = ConstructionPolicy::frozen_default();
    let mut consumer = Vec::new();
    let fixture = Timing::disabled("fixture", |root| {
        construct_bytes(
            policy,
            &policy.capacities(),
            &fixture_raw,
            &mut VecConsumer(&mut consumer),
            root.child("content"),
        )
    })
    .0
    .map_err(|error| format!("fixture construction failed: {error}"))?;
    if consumer.is_empty() {
        return Err("the demo fixture produced no canonical object".into());
    }
    println!(
        "fixture root: {} ({} bytes)",
        fixture.root,
        fixture_raw.len()
    );

    let store_path = options.store.as_ref().ok_or("--store is required for c2")?;
    let (save_result, save_report): (
        Result<(Store, layerfs_storage::SaveOutcome), StorageError>,
        TimingReport,
    ) = Timing::record("c2.only.save", |root| {
        let store = Store::create(
            store_path,
            StoragePolicy::frozen_default(),
            root.child("store.create"),
        )?;
        let mut operation = store.begin_save(root.child("storage.begin"))?;
        for object in consumer.iter() {
            operation.accept(object.clone(), root.child("storage.accept"))?;
        }
        let outcome = operation.finish(root.child("storage.finish"))?;
        Ok((store, outcome))
    });
    let (store, outcome) = save_result.map_err(|error| format!("save failed: {error}"))?;
    println!(
        "save result: inserted {} reused {}",
        outcome.inserted, outcome.reused
    );

    let (read_result, read_report) = Timing::record("c2.only.read", |read| {
        store.read_batch(&[fixture.root], read.child("storage.read"))
    });
    let (values, counters) = read_result.map_err(|error| format!("read failed: {error}"))?;
    if values.len() != 1 || values[0] != consumer[0].canonical() {
        return Err("readback does not match the supplied canonical object".into());
    }
    println!(
        "readback: verified {} object(s) from {} pack(s)",
        counters.objects, counters.packs_read
    );
    println!("exclusions: input acquisition, fixture construction and Store creation are separate scopes");
    render(&save_report);
    render(&read_report);
    save_timings(&save_report, &options.timings)
}

/// Integrated: real C1 construction handed to real C2 storage, then read back.
fn run_pipeline(options: &Options, bytes: &[u8]) -> Result<(), Failure> {
    let policy = ConstructionPolicy::frozen_default();
    let store_path = options
        .store
        .as_ref()
        .ok_or("--store is required for pipeline")?;
    let (result, report): (Result<PipelineResult, StorageError>, TimingReport) =
        Timing::record("c1c2.pipeline", |root| {
            let store = Store::create(
                store_path,
                StoragePolicy::frozen_default(),
                root.child("store.create"),
            )?;
            let mut operation = store.begin_save(root.child("storage.begin"))?;
            let mut handoff = SaveHandoff::new(&mut operation);
            let constructed = construct_stream(
                policy,
                &policy.capacities(),
                bytes,
                &mut handoff,
                root.child("content.construct"),
            );
            if let Some(failure) = handoff.take_failure() {
                return Err(failure);
            }
            let constructed = constructed?;
            let outcome = operation.finish(root.child("storage.finish"))?;
            let reader = StoreReader { store: &store };
            let mut out = Vec::new();
            read_all(
                &reader,
                constructed.root,
                &mut out,
                root.child("content.read"),
            )?;
            Ok((constructed, outcome, out))
        });
    let (constructed, outcome, out) =
        result.map_err(|error| format!("pipeline failed: {error}"))?;
    println!("mode: pipeline");
    println!("root: {}", constructed.root);
    println!("logical length: {}", constructed.logical_len);
    println!("readback bytes: {}", out.len());
    println!(
        "save result: inserted {} reused {} packs {}",
        outcome.inserted, outcome.reused, outcome.packs_created
    );
    if out != bytes {
        return Err("authenticated readback differs from the input".into());
    }
    println!("readback: verified byte-for-byte against the input");
    println!(
        "exclusions: C2 reuse decisions are inside the save scope; no warmed setup is claimed"
    );
    render(&report);
    save_timings(&report, &options.timings)
}

/// Independent read provider backed by a real Store connection.
struct StoreReader<'a> {
    store: &'a Store,
}

impl AuthenticatedObjects for StoreReader<'_> {
    fn read_canonical_batch(&self, ids: &[ObjectId]) -> ContentResult<Vec<Vec<u8>>> {
        let (values, _) = Timing::disabled("storage.read", |scope| {
            self.store.read_batch(ids, scope.child("storage.read"))
        })
        .0
        .map_err(|_: StorageError| ContentError::MissingObject)?;
        Ok(values)
    }
}

/// Consumer that keeps the prepared canonical objects for the C2-only mode.
struct VecConsumer<'a>(&'a mut Vec<layerfs_content::FinalizedObject>);

impl layerfs_content::FinalizedConsumer for VecConsumer<'_> {
    fn accept(&mut self, object: layerfs_content::FinalizedObject) -> ContentResult<()> {
        self.0.push(object);
        Ok(())
    }
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
    let bytes = read_input(&options.input)?;
    println!("input: {} ({} bytes)", options.input.display(), bytes.len());
    match options.mode {
        Mode::C1 => run_c1(&options, &bytes),
        Mode::C2 => run_c2(&options, &bytes),
        Mode::Pipeline => run_pipeline(&options, &bytes),
    }
}
