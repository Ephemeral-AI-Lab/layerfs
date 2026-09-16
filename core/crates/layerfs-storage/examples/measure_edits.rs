//! Real C1 edit, C2 storage-only and integrated edit-to-readback runs.
//!
//! This is a wiring and correctness demonstration, not a benchmark: one sample of
//! one deterministic fixture per case, a fresh output directory per run, and the
//! actual product result with the timing scopes the operation recorded. Timing
//! values are exploratory and are not a release qualification.
//!
//! Usage:
//!   measure_edits --mode c1|pipeline --case small --threshold-bytes 131072 --output FRESH_DIR
//!   measure_edits --mode c2 --case small --threshold-bytes 131072 --output FRESH_DIR
//!
//! Modes:
//!   c1       real edit through a supplied bounded authenticated provider and a
//!            non-persisting consumer; no database, pack or Store is opened
//!   c2       bounded supplied canonical objects -> save to acknowledgement ->
//!            authenticated read; no C1 file construction is timed
//!   pipeline real C1 edit handed to real C2 storage, then a fresh readback
//!
//! Cache state: the fixture is built by this process immediately before the timed
//! scope, so no warm-cache credit is claimed for any phase.

use std::fs::OpenOptions;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

use layerfs_content::{
    apply_edits, construct_bytes, read_all, AuthenticatedObjects, ConstructionPolicy, ContentError,
    ContentResult, DiscardingConsumer, Edit, EditRequest, EditStream, FinalizedConsumer,
    FinalizedObject, ObjectId, Replacements,
};
use layerfs_storage::{SaveHandoff, StorageError, StoragePolicy, Store};
use layerfs_telemetry::timer::{Timing, TimingReport};

type Failure = Box<dyn std::error::Error>;

/// Largest fixture this demonstration builds.
const FIXTURE_LIMIT: u64 = 4 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Mode {
    C1,
    C2,
    Pipeline,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Case {
    Small,
    Chunked,
    SmallToLarge,
    LargeToSmall,
    Batch,
}

impl Case {
    fn parse(value: &str) -> Result<Self, Failure> {
        match value {
            "small" => Ok(Self::Small),
            "chunked" => Ok(Self::Chunked),
            "small-to-large" => Ok(Self::SmallToLarge),
            "large-to-small" => Ok(Self::LargeToSmall),
            "batch" => Ok(Self::Batch),
            other => Err(format!("unsupported case {other}").into()),
        }
    }

    const fn name(self) -> &'static str {
        match self {
            Self::Small => "small",
            Self::Chunked => "chunked",
            Self::SmallToLarge => "small-to-large",
            Self::LargeToSmall => "large-to-small",
            Self::Batch => "batch",
        }
    }
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

    const fn name(self) -> &'static str {
        match self {
            Self::C1 => "c1",
            Self::C2 => "c2",
            Self::Pipeline => "pipeline",
        }
    }
}

struct Options {
    mode: Mode,
    case: Case,
    threshold: u64,
    output: PathBuf,
}

fn parse_options() -> Result<Options, Failure> {
    let mut mode = None;
    let mut case = None;
    let mut threshold = None;
    let mut output = None;
    let mut arguments = std::env::args().skip(1);
    while let Some(flag) = arguments.next() {
        let value = arguments
            .next()
            .ok_or_else(|| format!("{flag} needs a value"))?;
        match flag.as_str() {
            "--mode" => mode = Some(Mode::parse(&value)?),
            "--case" => case = Some(Case::parse(&value)?),
            "--threshold-bytes" => {
                threshold = Some(value.parse::<u64>().map_err(|_| "invalid threshold")?)
            }
            "--output" => output = Some(PathBuf::from(value)),
            other => return Err(format!("unsupported argument {other}").into()),
        }
    }
    let options = Options {
        mode: mode.ok_or("--mode is required")?,
        case: case.ok_or("--case is required")?,
        threshold: threshold.ok_or("--threshold-bytes is required")?,
        output: output.ok_or("--output is required")?,
    };
    if options.output.exists() {
        return Err(format!("--output {} already exists", options.output.display()).into());
    }
    let policy = ConstructionPolicy::new(options.threshold, 8, 4).validated()?;
    if policy.small_file_threshold_bytes() != options.threshold {
        return Err("threshold is not the accepted value".into());
    }
    std::fs::create_dir_all(&options.output)?;
    Ok(options)
}

fn noise(len: usize) -> Vec<u8> {
    let mut state = 0x9e37_79b9_7f4a_7c15_u64;
    (0..len)
        .map(|_| {
            state ^= state.wrapping_shl(7);
            state ^= state.wrapping_shr(9);
            state ^= state.wrapping_shl(8);
            state as u8
        })
        .collect()
}

/// One deterministic fixture: the base bytes, the edits and their replacements.
struct Fixture {
    base: Vec<u8>,
    edits: Vec<Edit>,
    replacements: Replacements,
}

fn fixture(case: Case, threshold: u64) -> Result<Fixture, Failure> {
    if threshold * 4 > FIXTURE_LIMIT {
        // The largest accepted cutoff still builds inside the declared limit.
        return Err(format!("threshold {threshold} exceeds the fixture budget").into());
    }
    let cutoff = threshold as usize;
    let (base, edits) = match case {
        Case::Small => (
            noise(cutoff / 2),
            vec![Edit::overwrite(cutoff as u64 / 4, cutoff as u64 / 4 + 512)],
        ),
        Case::Chunked => (
            noise(cutoff * 2),
            vec![Edit::overwrite(cutoff as u64, cutoff as u64 + 4_096)],
        ),
        Case::SmallToLarge => (noise(cutoff - 1), vec![Edit::insert(1_024, cutoff as u64)]),
        Case::LargeToSmall => (
            noise(cutoff * 2),
            vec![Edit::delete(cutoff as u64 / 2, cutoff as u64 * 3 / 2)],
        ),
        Case::Batch => {
            let base = noise(cutoff + cutoff / 2);
            let edits = vec![
                Edit::insert(1_000, 700),
                Edit::overwrite(cutoff as u64, cutoff as u64 + 300),
                Edit::delete(cutoff as u64 + 2_000, cutoff as u64 + 5_000),
            ];
            (base, edits)
        }
    };
    let mut replacements = Replacements::new();
    for edit in &edits {
        replacements.push(noise(edit.replacement_len() as usize));
    }
    Ok(Fixture {
        base,
        edits,
        replacements,
    })
}

/// Independent provider over a prepared object set.
struct Provider {
    objects: Vec<FinalizedObject>,
}

impl Provider {
    fn build(policy: ConstructionPolicy, bytes: &[u8]) -> Result<(Self, ObjectId), Failure> {
        let mut collector = Collector::default();
        let constructed = Timing::disabled("fixture", |root| {
            construct_bytes(
                policy,
                &policy.capacities(),
                bytes,
                &mut collector,
                root.child("content"),
            )
        })
        .0?;
        Ok((
            Self {
                objects: collector.objects,
            },
            constructed.root,
        ))
    }
}

impl AuthenticatedObjects for Provider {
    fn read_canonical_batch(&self, ids: &[ObjectId]) -> ContentResult<Vec<Vec<u8>>> {
        ids.iter()
            .map(|id| {
                self.objects
                    .iter()
                    .find(|object| object.id() == *id)
                    .map(|object| object.canonical().to_vec())
                    .ok_or(ContentError::MissingObject)
            })
            .collect()
    }
}

#[derive(Default)]
struct Collector {
    objects: Vec<FinalizedObject>,
}

impl FinalizedConsumer for Collector {
    fn accept(&mut self, object: FinalizedObject) -> ContentResult<()> {
        self.objects.push(object);
        Ok(())
    }
}

fn save_report(report: &TimingReport, path: &Path) -> Result<(), Failure> {
    if !report.has_root() {
        println!("timings: not recorded");
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
        println!("timings: INCOMPLETE - detail was clipped, not zero");
    }
    Ok(())
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

fn main() -> Result<(), Failure> {
    let options = parse_options()?;
    let policy = ConstructionPolicy::new(options.threshold, 8, 4).validated()?;
    let fixture = fixture(options.case, options.threshold)?;
    println!("mode: {}", options.mode.name());
    println!("case: {}", options.case.name());
    println!("threshold-bytes: {}", options.threshold);
    println!(
        "fixture: base {} bytes, {} edit(s), {} replacement bytes",
        fixture.base.len(),
        fixture.edits.len(),
        fixture
            .edits
            .iter()
            .map(|edit| edit.replacement_len())
            .sum::<u64>()
    );
    let stream = EditStream::new(fixture.base.len() as u64, fixture.edits.clone())?;
    println!(
        "stream: {} edit(s), base {} -> final {} bytes",
        stream.len(),
        stream.base_len(),
        stream.final_len()
    );
    match options.mode {
        Mode::C1 => run_c1(&options, policy, &fixture, &stream),
        Mode::C2 => run_c2(&options, policy, &fixture),
        Mode::Pipeline => run_pipeline(&options, policy, &fixture, &stream),
    }
}

/// C1 only: a real database-free edit with a supplied bounded provider.
fn run_c1(
    options: &Options,
    policy: ConstructionPolicy,
    fixture: &Fixture,
    stream: &EditStream,
) -> Result<(), Failure> {
    let (provider, root) = Provider::build(policy, &fixture.base)?;
    println!("base root: {root}");
    let mut consumer = DiscardingConsumer::new();
    let (result, report): (Result<_, ContentError>, TimingReport) =
        Timing::record("file.edit", |edit| {
            apply_edits(
                policy,
                &policy.capacities(),
                &provider,
                EditRequest {
                    root,
                    edits: stream,
                    source: &fixture.replacements,
                },
                &mut consumer,
                edit.child("edit.op"),
            )
        });
    let constructed = result?;
    println!("result root: {}", constructed.root);
    println!("result length: {}", constructed.logical_len);
    println!(
        "emitted: {} objects, {} canonical bytes",
        consumer.objects(),
        consumer.canonical_bytes()
    );
    println!(
        "scope: base reads, the no-op comparison, construction and consumer waiting are inside file.edit"
    );
    println!("exclusions: no database, pack, Store or file is opened in this mode");
    render(&report);
    save_report(&report, &options.output.join("c1-edit.json"))
}

/// C2 only: bounded supplied canonical objects through the real save and read.
fn run_c2(options: &Options, policy: ConstructionPolicy, fixture: &Fixture) -> Result<(), Failure> {
    // Two bounded supplied whole-file objects: a base and a near copy. The near
    // copy carries the base as an explicit predecessor, so a PREFIX record can be
    // selected without constructing any file here.
    let base_raw = &fixture.base[..fixture
        .base
        .len()
        .min(policy.capacities().whole_file_raw_limit)];
    let mut changed = base_raw.to_vec();
    let change = changed.len() / 4;
    let patch = changed.len().saturating_sub(change).min(256);
    let filler = noise(patch);
    changed[change..change + patch].copy_from_slice(&filler);
    let store_path = options.output.join("store.sqlite");
    let policy_row = StoragePolicy::new(1, options.threshold, 8, 4).validated()?;
    type SaveResult = Result<(Store, Vec<Vec<u8>>), StorageError>;
    let (result, report): (SaveResult, TimingReport) = Timing::record("storage.save", |save| {
        let store = Store::create(&store_path, policy_row, save.child("store.create"))?;
        let base = FinalizedObject::new(
            layerfs_content::ObjectRole::WholeFile,
            layerfs_content::file::encode_whole_file(&policy.capacities(), base_raw)?,
        )?;
        let base_id = base.id();
        let mut predecessors = layerfs_content::AdvisoryPredecessors::new();
        predecessors.push(
            base_id,
            layerfs_content::PredecessorProvenance::OriginalBase,
        )?;
        let dependent = FinalizedObject::new(
            layerfs_content::ObjectRole::WholeFile,
            layerfs_content::file::encode_whole_file(&policy.capacities(), &changed)?,
        )?
        .with_predecessors(predecessors);
        let dependent_id = dependent.id();
        let mut operation = store.begin_save(save.child("storage.begin"))?;
        operation.accept(base, save.child("storage.accept"))?;
        operation.accept(dependent, save.child("storage.accept"))?;
        let outcome = operation.finish(save.child("storage.finish"))?;
        println!(
            "save: inserted {} reused {} prefix records {} full records {} trials {}",
            outcome.inserted,
            outcome.reused,
            outcome.prefix_records,
            outcome.full_records,
            outcome.delta.trials
        );
        let (values, _) = store.read_batch(&[dependent_id], save.child("storage.read"))?;
        Ok((store, values))
    });
    let (store, values) = result?;
    println!("store: {}", store.path().display());
    let expected = layerfs_content::file::encode_whole_file(&policy.capacities(), &changed)?;
    if values.len() != 1 || values[0] != expected {
        return Err("authenticated readback differs from the supplied object".into());
    }
    println!(
        "readback: verified {} canonical bytes through an independent read wave",
        values[0].len()
    );
    println!("exclusions: no C1 file construction and no full-file object collection is timed");
    render(&report);
    save_report(&report, &options.output.join("c2-save.json"))
}

/// Integrated: the real C1 edit feeds the real C2 save, then a fresh readback.
fn run_pipeline(
    options: &Options,
    policy: ConstructionPolicy,
    fixture: &Fixture,
    stream: &EditStream,
) -> Result<(), Failure> {
    let store_path = options.output.join("store.sqlite");
    let policy_row = StoragePolicy::new(1, options.threshold, 8, 4).validated()?;
    // Base preparation is untimed and separate: an edit consumes an immutable
    // *stored* base, so the base save is acknowledged before the timed scope.
    let base_result: Result<(Store, ObjectId, layerfs_storage::SaveOutcome), StorageError> =
        Timing::disabled("base.prepare", |scope| {
            let (provider, root) = Provider::build(policy, &fixture.base)
                .map_err(|_| StorageError::Content(ContentError::Io))?;
            let store = Store::create(&store_path, policy_row, scope.child("store.create"))?;
            let mut operation = store.begin_save(scope.child("storage.begin"))?;
            for object in provider.objects {
                operation.accept(object, scope.child("storage.accept"))?;
            }
            let outcome = operation.finish(scope.child("storage.finish"))?;
            Ok((store, root, outcome))
        })
        .0;
    let (store, root, base_outcome) = base_result?;
    println!(
        "base preparation (untimed): root {} stored as {} object(s), {} pack(s), acknowledged {}",
        root, base_outcome.inserted, base_outcome.packs_created, base_outcome.acknowledged
    );
    let (result, report): (
        Result<(ObjectId, layerfs_storage::SaveOutcome), StorageError>,
        TimingReport,
    ) = Timing::record("edit.save", |scope| {
        // The base is read back through the real Store, exactly as a Workspace
        // would supply it: no in-memory copy of the base objects is required.
        let reader = StoreReader { store: &store };
        let mut operation = store.begin_save(scope.child("storage.begin"))?;
        let mut handoff = SaveHandoff::new(&mut operation);
        let constructed = apply_edits(
            policy,
            &policy.capacities(),
            &reader,
            EditRequest {
                root,
                edits: stream,
                source: &fixture.replacements,
            },
            &mut handoff,
            scope.child("file.edit"),
        );
        if let Some(failure) = handoff.take_failure() {
            return Err(failure);
        }
        let constructed = constructed?;
        let outcome = operation.finish(scope.child("storage.finish"))?;
        Ok((constructed.root, outcome))
    });
    let (edited_root, outcome) = result?;
    println!("edited root: {edited_root}");
    println!("edited length: {}", stream.final_len());
    println!(
        "save: acknowledged {} inserted {} reused {} packs {} prefix records {} full records {}",
        outcome.acknowledged,
        outcome.inserted,
        outcome.reused,
        outcome.packs_created,
        outcome.prefix_records,
        outcome.full_records
    );
    let (verify, verify_report): (Result<Vec<u8>, ContentError>, TimingReport) =
        Timing::record("verify.readback", |read| {
            let reader = StoreReader { store: &store };
            let mut out = Vec::new();
            read_all(&reader, edited_root, &mut out, read.child("content.read"))?;
            Ok(out)
        });
    let bytes = verify.map_err(|error| format!("readback failed: {error}"))?;
    println!("readback bytes: {}", bytes.len());
    let expected: Vec<u8> = {
        let mut model = fixture.base.clone();
        for (index, edit) in fixture.edits.iter().enumerate() {
            let mut replacement = Vec::new();
            let mut offset = 0_u64;
            while offset < edit.replacement_len() {
                let mut window = vec![0_u8; (edit.replacement_len() - offset).min(4096) as usize];
                let read = fixture.replacements.read_at(index, offset, &mut window)?;
                if read == 0 {
                    return Err("replacement source ended early".into());
                }
                replacement.extend_from_slice(&window[..read]);
                offset += read as u64;
            }
            model.splice(edit.start() as usize..edit.end() as usize, replacement);
        }
        model
    };
    if bytes != expected {
        return Err("authenticated readback differs from the independent model".into());
    }
    println!("readback: verified byte-for-byte against the independent model");
    println!(
        "scope: edit-to-save acknowledgement is edit.save; verification and readback are timed separately"
    );
    render(&report);
    render(&verify_report);
    save_report(&report, &options.output.join("pipeline-edit-save.json"))?;
    save_report(
        &verify_report,
        &options.output.join("pipeline-readback.json"),
    )
}

use layerfs_content::EditSource;

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
