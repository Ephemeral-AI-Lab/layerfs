//! Ingest throughput: payload -> C1 construction -> C2 save, one sample.
//!
//! The v0.1.7 case ladder stops at construction (`c1.construct.*` drives
//! `construct_bytes` and never opens a Store) and the `c2.*` families start at a
//! Store; nothing measures the whole path as one rate. This does: one
//! deterministic payload is built, put through the real C1 constructor, and then
//! saved through one `begin_save`/`accept`/`finish` into a real SQLite Store, with
//! each phase timed separately and reported in MiB/s.
//!
//! This is a **measurement, not a qualification**: one sample, one process, and
//! the fixture is built by this process immediately before the timed scopes, so no
//! warm-cache credit is claimed for any phase. The payload is dropped after
//! construction so the save phase does not carry two copies of it.
//!
//! Usage:
//!   measure_ingest --bytes 524288000 --output FRESH_DIR [--pattern noise|repeat]

use std::error::Error;
use std::path::PathBuf;
use std::time::Instant;

use layerfs_content::{
    construct_bytes, ConstructionPolicy, ContentResult, FinalizedConsumer, FinalizedObject,
    ObjectRole,
};
use layerfs_storage::{StoragePolicy, Store};
use layerfs_telemetry::timer::Timing;

type Failure = Box<dyn Error>;

const MIB: f64 = 1024.0 * 1024.0;
/// Default payload: the ladder's 500 MiB row.
const DEFAULT_BYTES: usize = 500 << 20;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Pattern {
    /// Incompressible bytes, so every chunk is stored in full.
    Noise,
    /// One 64 KiB noise block repeated, so chunks dedup and groups compress.
    Repeat,
}

struct Options {
    bytes: usize,
    output: PathBuf,
    pattern: Pattern,
}

fn parse_options() -> Result<Options, Failure> {
    let mut bytes = DEFAULT_BYTES;
    let mut output = None;
    let mut pattern = Pattern::Noise;
    let mut arguments = std::env::args().skip(1);
    while let Some(flag) = arguments.next() {
        let value = arguments
            .next()
            .ok_or_else(|| format!("{flag} needs a value"))?;
        match flag.as_str() {
            "--bytes" => bytes = value.parse()?,
            "--output" => output = Some(PathBuf::from(value)),
            "--pattern" => {
                pattern = match value.as_str() {
                    "noise" => Pattern::Noise,
                    "repeat" => Pattern::Repeat,
                    other => return Err(format!("unknown pattern {other}").into()),
                }
            }
            other => return Err(format!("unknown flag {other}").into()),
        }
    }
    Ok(Options {
        bytes,
        output: output.ok_or("--output is required")?,
        pattern,
    })
}

/// Deterministic splitmix64 bytes; the same payload on every run and every host.
fn noise(bytes: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes);
    let mut state = 0x9E37_79B9_7F4A_7C15_u64;
    while out.len() < bytes {
        state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        out.extend_from_slice(&(z ^ (z >> 31)).to_le_bytes());
    }
    out.truncate(bytes);
    out
}

fn payload(pattern: Pattern, bytes: usize) -> Vec<u8> {
    match pattern {
        Pattern::Noise => noise(bytes),
        Pattern::Repeat => {
            let block = noise(64 * 1024);
            block.iter().copied().cycle().take(bytes).collect()
        }
    }
}

/// Collects what construction emitted, so the save phase can replay it.
#[derive(Default)]
struct Collected {
    objects: Vec<FinalizedObject>,
    canonical_bytes: u64,
    chunks: u64,
}

impl FinalizedConsumer for Collected {
    fn accept(&mut self, object: FinalizedObject) -> ContentResult<()> {
        self.canonical_bytes = self
            .canonical_bytes
            .saturating_add(object.canonical().len() as u64);
        if object.role() == ObjectRole::Chunk {
            self.chunks += 1;
        }
        self.objects.push(object);
        Ok(())
    }
}

/// Wall nanoseconds of the four save sub-phases, disjoint and in order.
#[derive(Default)]
struct SavePhases {
    create_ns: u64,
    begin_ns: u64,
    accept_ns: u64,
    finish_ns: u64,
}

fn rate(bytes: usize, nanos: u64) -> f64 {
    if nanos == 0 {
        return 0.0;
    }
    (bytes as f64 / MIB) / (nanos as f64 / 1e9)
}

fn main() -> Result<(), Failure> {
    let options = parse_options()?;
    if options.output.exists() {
        return Err(format!(
            "{} exists; a run never overwrites evidence",
            options.output.display()
        )
        .into());
    }
    std::fs::create_dir_all(&options.output)?;
    let store_path = options.output.join("sample.sqlite");

    // Fixture: untimed, deterministic, declared. Built immediately before the
    // timed scopes and dropped before the save, so no phase is credited a warm
    // cache it did not pay for and the save does not carry two payloads.
    let started = Instant::now();
    let bytes = payload(options.pattern, options.bytes);
    let fixture_ns = started.elapsed().as_nanos() as u64;

    // Phase 1: C1 construction, the content-processing half.
    let construction = ConstructionPolicy::frozen_default();
    let capacities = construction.capacities();
    let mut collected = Collected::default();
    let started = Instant::now();
    let constructed = Timing::disabled("ingest.content", |scope| {
        construct_bytes(
            construction,
            &capacities,
            &bytes,
            &mut collected,
            scope.child("content.construct"),
        )
    })
    .0?;
    let content_ns = started.elapsed().as_nanos() as u64;
    drop(bytes);

    // Phase 2: C2 save, the storage and SQLite half, over the emitted objects.
    // Counted before the drain: the save consumes the objects it replays.
    let objects = collected.objects.len();
    let policy = StoragePolicy::frozen_default();
    let mut phases = SavePhases::default();
    let started = Instant::now();
    let outcome = Timing::disabled("ingest.save", |scope| -> Result<_, Failure> {
        let started = Instant::now();
        let store = Store::create(&store_path, policy, scope.child("store.create"))?;
        phases.create_ns = started.elapsed().as_nanos() as u64;
        let started = Instant::now();
        let mut operation = store.begin_save(scope.child("storage.begin"))?;
        phases.begin_ns = started.elapsed().as_nanos() as u64;
        let started = Instant::now();
        for object in collected.objects.drain(..) {
            operation.accept(object)?;
        }
        phases.accept_ns = started.elapsed().as_nanos() as u64;
        let started = Instant::now();
        let outcome = operation.finish(scope.child("storage.finish"))?;
        phases.finish_ns = started.elapsed().as_nanos() as u64;
        Ok(outcome)
    })
    .0?;
    let save_ns = started.elapsed().as_nanos() as u64;
    let store_bytes = std::fs::metadata(&store_path).map(|m| m.len()).unwrap_or(0);

    let total_ns = content_ns + save_ns;
    println!(
        "payload            {:.1} MiB ({} bytes, {:?})",
        options.bytes as f64 / MIB,
        options.bytes,
        options.pattern
    );
    println!(
        "fixture build      {:.3} s (untimed, declared)",
        fixture_ns as f64 / 1e9
    );
    println!(
        "objects emitted    {objects} ({} chunks), canonical {:.1} MiB",
        collected.chunks,
        collected.canonical_bytes as f64 / MIB
    );
    println!("root               {}", constructed.root);
    println!();
    println!(
        "content construct  {:>8.3} s   {:>8.2} MiB/s",
        content_ns as f64 / 1e9,
        rate(options.bytes, content_ns)
    );
    println!(
        "  store create     {:>8.3} s",
        phases.create_ns as f64 / 1e9
    );
    println!("  storage begin    {:>8.3} s", phases.begin_ns as f64 / 1e9);
    println!(
        "  accept (all)     {:>8.3} s   {:>8.2} MiB/s",
        phases.accept_ns as f64 / 1e9,
        rate(options.bytes, phases.accept_ns)
    );
    println!(
        "  storage finish   {:>8.3} s   (publication commit)",
        phases.finish_ns as f64 / 1e9
    );
    println!(
        "save total         {:>8.3} s   {:>8.2} MiB/s",
        save_ns as f64 / 1e9,
        rate(options.bytes, save_ns)
    );
    println!(
        "END TO END         {:>8.3} s   {:>8.2} MiB/s",
        total_ns as f64 / 1e9,
        rate(options.bytes, total_ns)
    );
    println!();
    println!(
        "store file         {} bytes ({:.1} MiB, {:.1}x the payload)",
        store_bytes,
        store_bytes as f64 / MIB,
        store_bytes as f64 / options.bytes as f64
    );
    println!("outcome            inserted={} reused={} packs_created={} pack_appends={} commits={} statements={}",
        outcome.inserted, outcome.reused, outcome.packs_created, outcome.pack_appends,
        outcome.commits, outcome.statements);
    Ok(())
}
