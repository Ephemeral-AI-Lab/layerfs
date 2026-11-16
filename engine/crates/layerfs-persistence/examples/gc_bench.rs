//! GC benchmark home: measure the `none` and `rebuild` compaction paths on a
//! generated SQLite Store and report wall time (downtime), the mark/build/
//! verify/install phases, peak temporary bytes, bytes written, per-GB rates,
//! cache state and the host class.
//!
//! Run it through the Cargo example target:
//!
//! ```text
//! cargo build -p layerfs-persistence --release --example gc_bench
//! cargo run -p layerfs-persistence --release --example gc_bench -- --scale small
//! ```
//!
//! `tools/ci/gc_capacity_final.sh` is the runbook entry point; it pins the
//! environment, checks free space, and refuses the owner-only 50 GB arm.
//!
//! Scales (the medium arm is opt-in; the final arm is owner-only and is NOT
//! run by CI or agents):
//!
//! | scale  | live bytes | runs | opt-in |
//! |--------|------------|------|--------|
//! | small  | ~30 MB     | 3    | none; the default |
//! | medium | ~10 GB     | 3    | `--scale medium` |
//! | final  | ~50 GB     | 1    | `GC_BENCH_OWNER=1 --owner-approved` |
//!
//! `--dry-run` prints the plan and allocates nothing. `--cold-cache` drops the
//! page cache before every measured run (records a refusal instead of failing
//! when the process lacks the privilege). `--keep` keeps the temporary Store.
//!
//! The fixture module is [`gc_bench/gc_fixture.rs`](gc_bench/gc_fixture.rs).
//! Only the Disposable (WAL/synchronous=OFF) profile is measured; owner
//! direction 2026-10-07 disabled Durable execution indefinitely.

#[path = "gc_bench/gc_fixture.rs"]
mod gc_fixture;

use gc_fixture::{FixtureSpec, PackStats};
use layerfs_persistence::compact_metrics::{CompactMetrics, CompactMetricsCapture};
use layerfs_persistence::{CompactPath, CompactRequest};
use std::error::Error;
use std::fmt;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const SCHEMA: &str = "gc-bench/1";
const OWNER_TARGET_CLASS: &str =
    "Dell PowerEdge R730 / PERC / rotational, ~500/600 MB/s sequential, not NVMe";
const SMALL_OBJECT_BYTES: usize = 40 * 1024;
const MAX_OBJECT_BYTES: usize = 131_071;
const GB: f64 = 1_000_000_000.0;

#[derive(Debug)]
struct Refusal(String);

impl fmt::Display for Refusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl Error for Refusal {}

type Result<T> = std::result::Result<T, Box<dyn Error>>;

fn refusal(message: impl Into<String>) -> Box<dyn Error> {
    Box::new(Refusal(message.into()))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Scale {
    Small,
    Medium,
    Final,
}

impl Scale {
    fn parse(value: &str) -> Option<Self> {
        match value {
            "small" => Some(Self::Small),
            "medium" => Some(Self::Medium),
            "final" => Some(Self::Final),
            _ => None,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Small => "small",
            Self::Medium => "medium",
            Self::Final => "final",
        }
    }

    fn spec(self) -> FixtureSpec {
        match self {
            Self::Small => FixtureSpec {
                groups: 16,
                group_size: 48,
                object_bytes: SMALL_OBJECT_BYTES,
            },
            Self::Medium => FixtureSpec {
                groups: 701,
                group_size: 99,
                object_bytes: MAX_OBJECT_BYTES,
            },
            // 3510 * 99 * 131071 = 45.56 GB live; ~8.8% dead takes the pack
            // total to ~50 GB, the T-G66 arm shape.
            Self::Final => FixtureSpec {
                groups: 3510,
                group_size: 99,
                object_bytes: MAX_OBJECT_BYTES,
            },
        }
    }

    fn default_runs(self) -> u64 {
        match self {
            Self::Small | Self::Medium => 3,
            Self::Final => 1,
        }
    }
}

#[derive(Debug)]
struct Options {
    scale: Scale,
    runs: u64,
    dry_run: bool,
    out: Option<PathBuf>,
    keep: bool,
    cold_cache: bool,
    tmp: Option<PathBuf>,
    owner_approved: bool,
    host_class: Option<String>,
    nominal_read_mbs: Option<u64>,
    nominal_write_mbs: Option<u64>,
}

fn env_value(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|value| !value.is_empty())
}

fn env_flag(key: &str) -> bool {
    matches!(env_value(key).as_deref(), Some("1" | "true" | "yes"))
}

fn env_u64(key: &str) -> Option<u64> {
    env_value(key).and_then(|value| value.parse().ok())
}

fn parse_options() -> Result<Option<Options>> {
    let mut scale = env_value("GC_BENCH_SCALE")
        .as_deref()
        .and_then(Scale::parse)
        .unwrap_or(Scale::Small);
    let mut runs = env_u64("GC_BENCH_RUNS").unwrap_or(0);
    let mut dry_run = env_flag("GC_BENCH_DRY_RUN");
    let mut out = env_value("GC_BENCH_OUT").map(PathBuf::from);
    let mut keep = env_flag("GC_BENCH_KEEP");
    let mut cold_cache = env_flag("GC_BENCH_COLD_CACHE");
    let mut tmp = env_value("GC_BENCH_TMPDIR").map(PathBuf::from);
    let mut owner_approved = env_flag("GC_BENCH_OWNER_APPROVED");
    let mut host_class = env_value("GC_BENCH_HOST_CLASS");
    let mut nominal_read_mbs = env_u64("GC_BENCH_NOMINAL_READ_MBS");
    let mut nominal_write_mbs = env_u64("GC_BENCH_NOMINAL_WRITE_MBS");

    let mut args = std::env::args().skip(1);
    while let Some(argument) = args.next() {
        let (key, inline) = match argument.split_once('=') {
            Some((key, value)) => (key.to_owned(), Some(value.to_owned())),
            None => (argument.clone(), None),
        };
        let mut value = |name: &str| -> Result<String> {
            inline
                .clone()
                .or_else(|| args.next())
                .ok_or_else(|| refusal(format!("{name} needs a value, for example {name}=small")))
        };
        match key.as_str() {
            "-h" | "--help" => {
                print_help();
                return Ok(None);
            }
            "--scale" => {
                let raw = value("--scale")?;
                scale = Scale::parse(&raw).ok_or_else(|| {
                    refusal(format!(
                        "unknown scale {raw:?}; expected small, medium or final"
                    ))
                })?;
            }
            "--runs" => {
                runs = value("--runs")?
                    .parse()
                    .map_err(|_| refusal("--runs takes a positive integer".to_owned()))?;
            }
            "--out" => out = Some(PathBuf::from(value("--out")?)),
            "--tmpdir" => tmp = Some(PathBuf::from(value("--tmpdir")?)),
            "--keep" => keep = true,
            "--cold-cache" => cold_cache = true,
            "--owner-approved" => owner_approved = true,
            "--dry-run" => dry_run = true,
            "--host-class" => host_class = Some(value("--host-class")?),
            "--nominal-read-mbs" => {
                nominal_read_mbs = Some(
                    value("--nominal-read-mbs")?
                        .parse()
                        .map_err(|_| refusal("--nominal-read-mbs takes an integer".to_owned()))?,
                );
            }
            "--nominal-write-mbs" => {
                nominal_write_mbs = Some(
                    value("--nominal-write-mbs")?
                        .parse()
                        .map_err(|_| refusal("--nominal-write-mbs takes an integer".to_owned()))?,
                );
            }
            other => {
                return Err(refusal(format!(
                    "unknown option {other:?}; run with --help"
                )))
            }
        }
    }

    let runs = if runs == 0 {
        scale.default_runs()
    } else {
        runs
    };
    Ok(Some(Options {
        scale,
        runs,
        dry_run,
        out,
        keep,
        cold_cache,
        tmp,
        owner_approved,
        host_class,
        nominal_read_mbs,
        nominal_write_mbs,
    }))
}

fn print_help() {
    println!(
        "gc_bench: measure the `none` and `rebuild` GC paths on a generated Store\n\
         \n\
         usage: gc_bench [--scale small|medium|final] [--runs N] [--out FILE]\n\
         \x20              [--tmpdir DIR] [--cold-cache] [--keep] [--dry-run]\n\
         \x20              [--owner-approved] [--host-class NAME]\n\
         \x20              [--nominal-read-mbs N] [--nominal-write-mbs N]\n\
         \n\
         small   ~30 MB fixture, 3 runs per arm; the default\n\
         medium  ~10 GB fixture, 3 runs per arm; opt-in (runbook --medium)\n\
         final   ~50 GB fixture, 1 run per arm; owner-only and NOT run by agents\n\
         \n\
         environment: GC_BENCH_SCALE GC_BENCH_RUNS GC_BENCH_OUT GC_BENCH_TMPDIR\n\
         \x20            GC_BENCH_KEEP GC_BENCH_COLD_CACHE GC_BENCH_DRY_RUN\n\
         \x20            GC_BENCH_GROUPS GC_BENCH_GROUP GC_BENCH_OBJECT_BYTES\n\
         \x20            GC_BENCH_DEAD_SHARE (permille, default 88)\n\
         \x20            GC_BENCH_OWNER=1 GC_BENCH_OWNER_APPROVED=1 (final arm only)\n\
         \x20            GC_BENCH_HOST_CLASS GC_BENCH_NOMINAL_READ_MBS\n\
         \x20            GC_BENCH_NOMINAL_WRITE_MBS (declared host class)\n"
    );
}

fn min(values: &[u64]) -> u64 {
    values.iter().copied().min().unwrap_or(0)
}

fn max(values: &[u64]) -> u64 {
    values.iter().copied().max().unwrap_or(0)
}

fn median(values: &[u64]) -> u64 {
    if values.is_empty() {
        return 0;
    }
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    sorted[sorted.len() / 2]
}

fn json_string(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            control if control < ' ' => {
                let _ = write!(out, "\\u{:04x}", control as u32);
            }
            other => out.push(other),
        }
    }
    out.push('"');
    out
}

#[derive(Clone, Copy, Debug, Default)]
struct ProcIo {
    rchar: u64,
    wchar: u64,
    read_bytes: u64,
    write_bytes: u64,
}

fn proc_io() -> ProcIo {
    let mut io = ProcIo::default();
    let Ok(text) = std::fs::read_to_string("/proc/self/io") else {
        return io;
    };
    for line in text.lines() {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let Ok(value) = value.trim().parse::<u64>() else {
            continue;
        };
        match key {
            "rchar" => io.rchar = value,
            "wchar" => io.wchar = value,
            "read_bytes" => io.read_bytes = value,
            "write_bytes" => io.write_bytes = value,
            _ => {}
        }
    }
    io
}

#[derive(Clone, Debug, Default)]
struct CacheDrop {
    dropped: bool,
    note: String,
}

fn drop_caches() -> CacheDrop {
    let _ = Command::new("sync").status();
    match std::fs::write("/proc/sys/vm/drop_caches", "3\n") {
        Ok(()) => CacheDrop {
            dropped: true,
            note: "sync + echo 3 > /proc/sys/vm/drop_caches".to_owned(),
        },
        Err(error) => CacheDrop {
            dropped: false,
            note: format!(
                "sync + echo 3 > /proc/sys/vm/drop_caches refused ({error}); the run is warm"
            ),
        },
    }
}

#[derive(Clone, Debug, Default)]
struct TempFootprint {
    total: u64,
    staged: u64,
    journal: u64,
    visited: u64,
    rewrite: u64,
}

/// Samples the run's temp artifacts from a second thread. The staged file,
/// its `-journal` and the visited scratch are visible in the Store's
/// directory; SQLite writes the rewrite's temp database to the process temp
/// directory and unlinks it, so it is reachable only through `/proc/self/fd`.
struct FootprintSampler {
    stop: Arc<AtomicBool>,
    handle: Option<std::thread::JoinHandle<()>>,
    total: Arc<AtomicU64>,
    staged: Arc<AtomicU64>,
    journal: Arc<AtomicU64>,
    visited: Arc<AtomicU64>,
    rewrite: Arc<AtomicU64>,
}

impl FootprintSampler {
    fn start(store: &Path) -> Self {
        let name = store
            .file_name()
            .map(|file| file.to_string_lossy().into_owned())
            .unwrap_or_default();
        let directory = store
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."));
        let stop = Arc::new(AtomicBool::new(false));
        let total = Arc::new(AtomicU64::new(0));
        let staged = Arc::new(AtomicU64::new(0));
        let journal = Arc::new(AtomicU64::new(0));
        let visited = Arc::new(AtomicU64::new(0));
        let rewrite = Arc::new(AtomicU64::new(0));
        let handle = {
            let stop = Arc::clone(&stop);
            let total = Arc::clone(&total);
            let staged_at = Arc::clone(&staged);
            let journal_at = Arc::clone(&journal);
            let visited_at = Arc::clone(&visited);
            let rewrite_at = Arc::clone(&rewrite);
            std::thread::spawn(move || {
                while !stop.load(Ordering::Relaxed) {
                    let (mut staged, mut journal, mut visited) = (0u64, 0u64, 0u64);
                    if let Ok(entries) = std::fs::read_dir(&directory) {
                        for entry in entries.flatten() {
                            let file = entry.file_name().to_string_lossy().into_owned();
                            let Some(suffix) = file.strip_prefix(&name) else {
                                continue;
                            };
                            let Some(suffix) = suffix.strip_prefix('.') else {
                                continue;
                            };
                            let size = entry.metadata().map(|meta| meta.len()).unwrap_or(0);
                            if suffix == "compact" {
                                staged = staged.max(size);
                            } else if suffix.starts_with("compact-journal")
                                || suffix.starts_with("compact-mj")
                            {
                                journal = journal.max(size);
                            } else if suffix.starts_with("compact-visited") {
                                visited = visited.max(size);
                            }
                        }
                    }
                    let mut rewrite = 0u64;
                    if let Ok(fds) = std::fs::read_dir("/proc/self/fd") {
                        for fd in fds.flatten() {
                            let Ok(link) = std::fs::read_link(fd.path()) else {
                                continue;
                            };
                            if !link.to_string_lossy().contains("etilqs_") {
                                continue;
                            }
                            if let Ok(meta) = std::fs::metadata(fd.path()) {
                                rewrite = rewrite.max(meta.len());
                            }
                        }
                    }
                    staged_at.fetch_max(staged, Ordering::Relaxed);
                    journal_at.fetch_max(journal, Ordering::Relaxed);
                    visited_at.fetch_max(visited, Ordering::Relaxed);
                    rewrite_at.fetch_max(rewrite, Ordering::Relaxed);
                    total.fetch_max(staged + journal + visited + rewrite, Ordering::Relaxed);
                    std::thread::sleep(Duration::from_micros(200));
                }
            })
        };
        Self {
            stop,
            handle: Some(handle),
            total,
            staged,
            journal,
            visited,
            rewrite,
        }
    }

    fn finish(mut self) -> TempFootprint {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
        TempFootprint {
            total: self.total.load(Ordering::Relaxed),
            staged: self.staged.load(Ordering::Relaxed),
            journal: self.journal.load(Ordering::Relaxed),
            visited: self.visited.load(Ordering::Relaxed),
            rewrite: self.rewrite.load(Ordering::Relaxed),
        }
    }
}

#[derive(Clone, Debug)]
struct Measured {
    wall_micros: u64,
    path: &'static str,
    generation_before: u64,
    generation_after: u64,
    dead_bytes_bound: u64,
    dead_bytes_refined: u64,
    physical_shrink: u64,
    peak_temp_bytes: u64,
    old_file_bytes: u64,
    new_file_bytes: u64,
    added_objects: u64,
    pack_before: PackStats,
    pack_after: PackStats,
    phases: Vec<(String, u64)>,
    sampled: TempFootprint,
    io: ProcIo,
    cache: CacheDrop,
}

impl Measured {
    fn stored_bytes(&self) -> u64 {
        self.pack_before.total
    }

    fn live_bytes(&self) -> u64 {
        self.pack_before.total.saturating_sub(self.dead_bytes_bound)
    }

    fn phase_nanos(&self, phase: &str) -> u64 {
        self.phases
            .iter()
            .find(|(name, _)| name == phase)
            .map(|(_, value)| *value)
            .unwrap_or(0)
    }

    fn copy_bytes(&self) -> u64 {
        if self.path == "rebuild" {
            self.new_file_bytes
        } else {
            0
        }
    }

    fn bytes_copied_per_freed_byte(&self) -> f64 {
        if self.physical_shrink == 0 {
            0.0
        } else {
            self.copy_bytes() as f64 / self.physical_shrink as f64
        }
    }
}

fn phase_nanos(metrics: &CompactMetrics) -> Vec<(String, u64)> {
    let mut phases: Vec<(String, u64)> = metrics
        .samples()
        .into_iter()
        .filter(|sample| sample.name == "compact.phase.duration_ns")
        .filter(|sample| sample.label("statistic") == Some("sum"))
        .filter_map(|sample| Some((sample.label("phase")?.to_owned(), sample.value)))
        .collect();
    phases.sort_by(|left, right| left.0.cmp(&right.0));
    phases
}

fn request(op: u8) -> CompactRequest {
    CompactRequest {
        op_id: [op; 16],
        ..CompactRequest::default()
    }
}

fn measure(store: &Path, op: u8, added_objects: u64, cold_cache: bool) -> Result<Measured> {
    let handles = gc_fixture::reopen(store);
    let pack_before = gc_fixture::pack_stats(store);
    let old_file_bytes = gc_fixture::physical_bytes(store);

    let cache = if cold_cache {
        drop_caches()
    } else {
        CacheDrop {
            dropped: false,
            note: "warm: --cold-cache not requested".to_owned(),
        }
    };

    let io_before = proc_io();
    let sampler = FootprintSampler::start(store);
    let capture = CompactMetricsCapture::start();
    let started = Instant::now();
    let receipt = handles
        .compact(&request(op))
        .map_err(|error| Refusal(format!("compaction failed: {error:?}")))?;
    let wall_micros = u64::try_from(started.elapsed().as_micros()).unwrap_or(u64::MAX);
    let metrics: CompactMetrics = capture.finish();
    let io_after = proc_io();
    let sampled = sampler.finish();
    let new_file_bytes = gc_fixture::physical_bytes(store);
    let pack_after = gc_fixture::pack_stats(store);

    Ok(Measured {
        wall_micros,
        path: match receipt.path {
            CompactPath::None => "none",
            CompactPath::Rebuild => "rebuild",
        },
        generation_before: receipt.generation_before,
        generation_after: receipt.generation_after,
        dead_bytes_bound: receipt.dead_bytes_bound,
        dead_bytes_refined: receipt.dead_bytes_refined,
        physical_shrink: receipt.physical_shrink,
        peak_temp_bytes: receipt.peak_temp_bytes,
        old_file_bytes,
        new_file_bytes,
        added_objects,
        pack_before,
        pack_after,
        phases: phase_nanos(&metrics),
        sampled,
        io: ProcIo {
            rchar: io_after.rchar.saturating_sub(io_before.rchar),
            wchar: io_after.wchar.saturating_sub(io_before.wchar),
            read_bytes: io_after.read_bytes.saturating_sub(io_before.read_bytes),
            write_bytes: io_after.write_bytes.saturating_sub(io_before.write_bytes),
        },
        cache,
    })
}

fn median_phase(runs: &[Measured], phase: &str) -> u64 {
    median(
        &runs
            .iter()
            .map(|run| run.phase_nanos(phase))
            .collect::<Vec<_>>(),
    )
}

fn per_gb_object(runs: &[Measured]) -> String {
    let stored = median(&runs.iter().map(Measured::stored_bytes).collect::<Vec<_>>());
    let live = median(&runs.iter().map(Measured::live_bytes).collect::<Vec<_>>());
    let new = median(
        &runs
            .iter()
            .map(|run| run.new_file_bytes)
            .collect::<Vec<_>>(),
    );
    let wall = median(&runs.iter().map(|run| run.wall_micros).collect::<Vec<_>>());
    let mark = median_phase(runs, "mark");
    let build = median_phase(runs, "build");
    let verify = median_phase(runs, "verify");
    let install = median_phase(runs, "install");
    let copy_ratio = median(
        &runs
            .iter()
            .map(|run| {
                if run.physical_shrink == 0 {
                    0
                } else {
                    run.new_file_bytes.saturating_mul(1000) / run.physical_shrink
                }
            })
            .collect::<Vec<_>>(),
    ) as f64
        / 1000.0;
    format!(
        "{{\n      \"wallSecondsPerStoredGb\": {:.3},\n      \
         \"wallSecondsPerLiveGb\": {:.3},\n      \
         \"markSecondsPerStoredGb\": {:.3},\n      \
         \"markSecondsPerLiveGb\": {:.3},\n      \
         \"markMBs\": {:.1},\n      \
         \"buildSecondsPerNewGb\": {:.3},\n      \
         \"verifySecondsPerNewGb\": {:.3},\n      \
         \"installSeconds\": {:.3},\n      \
         \"buildMBs\": {:.1},\n      \
         \"newBytesPerFreedByte\": {:.3}\n    }}",
        seconds_per_micros_to_gb(wall, stored),
        seconds_per_micros_to_gb(wall, live),
        seconds_per_nanos_to_gb(mark, stored),
        seconds_per_nanos_to_gb(mark, live),
        if mark == 0 {
            0.0
        } else {
            live as f64 / 1e6 / (mark as f64 / 1e9)
        },
        if new == 0 {
            0.0
        } else {
            seconds_per_nanos_to_gb(build, new)
        },
        if new == 0 {
            0.0
        } else {
            seconds_per_nanos_to_gb(verify, new)
        },
        install as f64 / 1e9,
        if build == 0 || new == 0 {
            0.0
        } else {
            new as f64 / 1e6 / (build as f64 / 1e9)
        },
        copy_ratio,
    )
}

fn seconds_per_micros_to_gb(micros: u64, bytes: u64) -> f64 {
    if bytes == 0 {
        return 0.0;
    }
    (micros as f64 / 1e6) / (bytes as f64 / GB)
}

fn seconds_per_nanos_to_gb(nanos: u64, bytes: u64) -> f64 {
    if bytes == 0 {
        return 0.0;
    }
    (nanos as f64 / 1e9) / (bytes as f64 / GB)
}

fn json_run(index: u64, run: &Measured) -> String {
    format!(
        "{{\n      \"index\": {index},\n      \"path\": \"{}\",\n      \
         \"wallMicros\": {},\n      \"wallSeconds\": {:.6},\n      \"downtimeSeconds\": {:.6},\n      \
         \"generationBefore\": {},\n      \"generationAfter\": {},\n      \
         \"storedBytes\": {},\n      \"liveBytes\": {},\n      \
         \"deadBytesBound\": {},\n      \"deadBytesRefined\": {},\n      \
         \"physicalShrink\": {},\n      \"newFileBytes\": {},\n      \
         \"oldFileBytes\": {},\n      \"addedDeadObjects\": {},\n      \
         \"peakTempBytes\": {},\n      \"sampledPeakTempBytes\": {},\n      \
         \"sampledTemp\": {{\"staged\": {}, \"journal\": {}, \"visited\": {}, \
         \"rewrite\": {}, \"total\": {}}},\n      \
         \"phasesNanos\": {{\"mark\": {}, \"build\": {}, \"verify\": {}, \"install\": {}}},\n      \
         \"processIo\": {{\"writeBytes\": {}, \"wchar\": {}, \"readBytes\": {}, \
         \"rchar\": {}}},\n      \
         \"copyBytes\": {},\n      \"bytesCopiedPerFreedByte\": {:.3},\n      \
         \"cacheDropped\": {},\n      \"packCountBefore\": {},\n      \
         \"packCountAfter\": {},\n      \"phaseTotalNanos\": {}\n    }}",
        run.path,
        run.wall_micros,
        run.wall_micros as f64 / 1e6,
        run.wall_micros as f64 / 1e6,
        run.generation_before,
        run.generation_after,
        run.stored_bytes(),
        run.live_bytes(),
        run.dead_bytes_bound,
        run.dead_bytes_refined,
        run.physical_shrink,
        run.new_file_bytes,
        run.old_file_bytes,
        run.added_objects,
        run.peak_temp_bytes,
        run.sampled.total,
        run.sampled.staged,
        run.sampled.journal,
        run.sampled.visited,
        run.sampled.rewrite,
        run.sampled.total,
        run.phase_nanos("mark"),
        run.phase_nanos("build"),
        run.phase_nanos("verify"),
        run.phase_nanos("install"),
        run.io.write_bytes,
        run.io.wchar,
        run.io.read_bytes,
        run.io.rchar,
        run.copy_bytes(),
        run.bytes_copied_per_freed_byte(),
        run.cache.dropped,
        run.pack_before.count,
        run.pack_after.count,
        run.phases.iter().map(|(_, value)| *value).sum::<u64>(),
    )
}

fn json_arm(name: &str, runs: &[Measured]) -> String {
    let wall: Vec<u64> = runs.iter().map(|run| run.wall_micros).collect();
    let peak: Vec<u64> = runs.iter().map(|run| run.peak_temp_bytes).collect();
    let shrink: Vec<u64> = runs.iter().map(|run| run.physical_shrink).collect();
    let new: Vec<u64> = runs.iter().map(|run| run.new_file_bytes).collect();
    let written: Vec<u64> = runs.iter().map(|run| run.io.write_bytes).collect();
    let runs_json = runs
        .iter()
        .enumerate()
        .map(|(index, run)| json_run(index as u64 + 1, run))
        .collect::<Vec<_>>()
        .join(",\n    ");
    format!(
        "\n  \"{name}\": {{\n    \"path\": \"{}\",\n    \"runs\": [\n    {}\n    ],\n    \
         \"median\": {{\n      \"wallMicros\": {},\n      \"wallSeconds\": {:.6},\n      \"downtimeSeconds\": {:.6},\n      \
         \"peakTempBytes\": {},\n      \"physicalShrink\": {},\n      \
         \"newFileBytes\": {},\n      \"processWriteBytes\": {}\n    }},\n    \
         \"minWallMicros\": {},\n    \"maxWallMicros\": {},\n    \
         \"phasesMedianNanos\": {{\"mark\": {}, \"build\": {}, \"verify\": {}, \
         \"install\": {}}},\n    \"perGb\": {}\n  }}",
        runs.first().map(|run| run.path).unwrap_or(name),
        runs_json,
        median(&wall),
        median(&wall) as f64 / 1e6,
        median(&wall) as f64 / 1e6,
        median(&peak),
        median(&shrink),
        median(&new),
        median(&written),
        min(&wall),
        max(&wall),
        median_phase(runs, "mark"),
        median_phase(runs, "build"),
        median_phase(runs, "verify"),
        median_phase(runs, "install"),
        per_gb_object(runs),
    )
}

fn read_trim(path: &Path) -> Option<String> {
    std::fs::read_to_string(path)
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

#[derive(Clone, Debug)]
struct DiskInfo {
    name: String,
    model: String,
    rotational: bool,
    size_bytes: u64,
}

#[derive(Clone, Debug)]
struct HostInfo {
    product: String,
    cpu_model: String,
    kernel: String,
    cpus: u64,
    mem_bytes: u64,
    tmp_filesystem: String,
    host_class: String,
    host_class_source: String,
    detected_host_class: String,
    storage_media: String,
    detected_storage_media: String,
    not_nvme: bool,
    nominal_read_mbs: u64,
    nominal_write_mbs: u64,
    disks: Vec<DiskInfo>,
}

fn cpu_model() -> String {
    std::fs::read_to_string("/proc/cpuinfo")
        .ok()
        .and_then(|text| {
            text.lines().find_map(|line| {
                line.strip_prefix("model name")
                    .and_then(|rest| rest.split_once(':'))
                    .map(|(_, value)| value.trim().to_owned())
            })
        })
        .unwrap_or_else(|| "unknown".to_owned())
}

fn mem_bytes() -> u64 {
    std::fs::read_to_string("/proc/meminfo")
        .ok()
        .and_then(|text| {
            text.lines().find_map(|line| {
                let rest = line.strip_prefix("MemTotal:")?;
                let kib: u64 = rest.trim().trim_end_matches(" kB").trim().parse().ok()?;
                Some(kib.saturating_mul(1024))
            })
        })
        .unwrap_or(0)
}

fn tmp_filesystem(path: &Path) -> String {
    Command::new("stat")
        .args(["-f", "-c", "%T"])
        .arg(path)
        .output()
        .ok()
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "unknown".to_owned())
}

fn disks() -> Vec<DiskInfo> {
    let mut disks = Vec::new();
    let Ok(entries) = std::fs::read_dir("/sys/block") else {
        return disks;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with("loop")
            || name.starts_with("ram")
            || name.starts_with("zram")
            || name.starts_with("sr")
        {
            continue;
        }
        let base = entry.path();
        let model = read_trim(&base.join("device/model")).unwrap_or_else(|| "unknown".to_owned());
        let rotational = read_trim(&base.join("queue/rotational"))
            .map(|value| value != "0")
            .unwrap_or(false);
        let size_bytes = read_trim(&base.join("size"))
            .and_then(|value| value.parse::<u64>().ok())
            .map(|sectors| sectors.saturating_mul(512))
            .unwrap_or(0);
        disks.push(DiskInfo {
            name,
            model,
            rotational,
            size_bytes,
        });
    }
    disks.sort_by(|left, right| left.name.cmp(&right.name));
    disks
}

#[derive(Clone, Debug, Default)]
struct HostOverrides {
    class: Option<String>,
    nominal_read_mbs: Option<u64>,
    nominal_write_mbs: Option<u64>,
}

fn host_info(tmp: &Path, overrides: &HostOverrides) -> HostInfo {
    let product = read_trim(Path::new("/sys/class/dmi/id/product_name"))
        .unwrap_or_else(|| "unknown".to_owned());
    let disks = disks();
    let has_nvme = disks.iter().any(|disk| disk.name.starts_with("nvme"));
    let rotational = disks
        .iter()
        .any(|disk| disk.rotational && !disk.name.starts_with("dm-"));
    let perc = disks
        .iter()
        .any(|disk| disk.model.to_ascii_uppercase().contains("PERC"));
    let detected_media = if has_nvme {
        "nvme"
    } else if rotational {
        "rotational"
    } else {
        "ssd-or-unknown"
    };
    let r730 = product.to_ascii_lowercase().contains("r730");
    let detected_class = if r730 {
        format!(
            "Dell PowerEdge R730 / {}{} (owner target class)",
            if perc { "PERC / " } else { "" },
            detected_media
        )
    } else if detected_media == "rotational" {
        format!("{product} / rotational")
    } else {
        format!("{product} / {detected_media}")
    };
    let declared = overrides
        .class
        .clone()
        .filter(|value| !value.trim().is_empty());
    let (host_class, host_class_source) = match declared {
        Some(class) => (class, "declared".to_owned()),
        None => (detected_class.clone(), "detected".to_owned()),
    };
    let lowered = host_class.to_ascii_lowercase();
    // "not NVMe" must not be mistaken for an NVMe declaration.
    let explicitly_not_nvme = lowered.contains("not nvme") || lowered.contains("non-nvme");
    let says_nvme = !explicitly_not_nvme && lowered.contains("nvme");
    let says_rotational = lowered.contains("rotational");
    let storage_media = if says_nvme {
        "nvme"
    } else if says_rotational {
        "rotational"
    } else {
        detected_media
    };
    let not_nvme = if says_nvme {
        false
    } else if explicitly_not_nvme || says_rotational {
        true
    } else {
        !has_nvme
    };
    let nominal_read_mbs = overrides
        .nominal_read_mbs
        .unwrap_or(if r730 { 500 } else { 0 });
    let nominal_write_mbs = overrides
        .nominal_write_mbs
        .unwrap_or(if r730 { 600 } else { 0 });
    HostInfo {
        product,
        cpu_model: cpu_model(),
        kernel: read_trim(Path::new("/proc/sys/kernel/osrelease"))
            .unwrap_or_else(|| "unknown".to_owned()),
        cpus: std::thread::available_parallelism()
            .map(|value| u64::try_from(value.get()).unwrap_or(0))
            .unwrap_or(0),
        mem_bytes: mem_bytes(),
        tmp_filesystem: tmp_filesystem(tmp),
        host_class,
        host_class_source,
        detected_host_class: detected_class,
        storage_media: storage_media.to_owned(),
        detected_storage_media: detected_media.to_owned(),
        not_nvme,
        nominal_read_mbs,
        nominal_write_mbs,
        disks,
    }
}

fn available_bytes(path: &Path) -> u64 {
    let output = match Command::new("df").args(["-P", "-B1"]).arg(path).output() {
        Ok(output) => output,
        Err(_) => return 0,
    };
    let text = String::from_utf8_lossy(&output.stdout);
    text.lines()
        .last()
        .and_then(|line| line.split_whitespace().nth(3))
        .and_then(|value| value.parse().ok())
        .unwrap_or(0)
}

fn json_host(host: &HostInfo) -> String {
    let disks = host
        .disks
        .iter()
        .map(|disk| {
            format!(
                "\n      {{\"name\": \"{}\", \"model\": \"{}\", \"rotational\": {}, \
                 \"sizeBytes\": {}}}",
                json_string(&disk.name),
                json_string(&disk.model),
                disk.rotational,
                disk.size_bytes
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    format!(
        "{{\n    \"product\": {},\n    \"cpuModel\": {},\n    \"kernel\": {},\n    \
         \"cpus\": {},\n    \"memBytes\": {},\n    \"tmpFilesystem\": {},\n    \
         \"hostClass\": {},\n    \"hostClassSource\": {},\n    \"detectedHostClass\": {},\n    \
         \"ownerTargetClass\": {},\n    \"storageMedia\": {},\n    \
         \"detectedStorageMedia\": {},\n    \"notNvme\": {},\n    \
         \"nominalSequentialMBs\": {{\"read\": {}, \"write\": {}}},\n    \
         \"disks\": [{}]\n  }}",
        json_string(&host.product),
        json_string(&host.cpu_model),
        json_string(&host.kernel),
        host.cpus,
        host.mem_bytes,
        json_string(&host.tmp_filesystem),
        json_string(&host.host_class),
        json_string(&host.host_class_source),
        json_string(&host.detected_host_class),
        json_string(OWNER_TARGET_CLASS),
        json_string(&host.storage_media),
        json_string(&host.detected_storage_media),
        host.not_nvme,
        host.nominal_read_mbs,
        host.nominal_write_mbs,
        disks
    )
}

fn scratch_dir(prefix: &str, root: Option<&Path>, _keep: bool) -> Result<PathBuf> {
    let base = root
        .map(Path::to_path_buf)
        .unwrap_or_else(std::env::temp_dir);
    std::fs::create_dir_all(&base)
        .map_err(|error| refusal(format!("create {}: {error}", base.display())))?;
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let path = base.join(format!("gc-bench-{prefix}-{}-{nanos}", std::process::id()));
    std::fs::create_dir_all(&path)
        .map_err(|error| refusal(format!("create {}: {error}", path.display())))?;
    Ok(path)
}

fn cleanup(path: &Path, keep: bool) {
    if keep {
        eprintln!("gc_bench: kept {}", path.display());
    } else {
        let _ = std::fs::remove_dir_all(path);
    }
}

fn emit(json: &str, out: &Option<PathBuf>) -> Result<()> {
    println!("{json}");
    if let Some(path) = out {
        std::fs::write(path, format!("{json}\n"))
            .map_err(|error| refusal(format!("write {}: {error}", path.display())))?;
        eprintln!("gc_bench: wrote {}", path.display());
    }
    Ok(())
}

fn main() {
    match run() {
        Ok(()) => {}
        Err(error) => {
            eprintln!("gc_bench: {error}");
            std::process::exit(2);
        }
    }
}

fn run() -> Result<()> {
    let Some(options) = parse_options()? else {
        return Ok(());
    };
    let scale = options.scale;
    let spec = scale.spec();
    let runs = options.runs.max(1);
    let owner = env_value("GC_BENCH_OWNER").as_deref() == Some("1");

    if scale == Scale::Final && !options.dry_run {
        if !owner {
            return Err(refusal(
                "the 50 GB final arm is owner-only: set GC_BENCH_OWNER=1 and pass \
                 --owner-approved; agents and CI must not run it",
            ));
        }
        if !options.owner_approved {
            return Err(refusal(
                "the 50 GB final arm needs the explicit --owner-approved flag in addition \
                 to GC_BENCH_OWNER=1",
            ));
        }
        if runs != 1 {
            return Err(refusal(
                "the 50 GB final arm is one run per arm (section 13); pass --runs 1",
            ));
        }
    }

    let groups = env_u64("GC_BENCH_GROUPS").unwrap_or(spec.groups);
    let group_size = env_u64("GC_BENCH_GROUP").unwrap_or(spec.group_size);
    let object_bytes =
        usize::try_from(env_u64("GC_BENCH_OBJECT_BYTES").unwrap_or(spec.object_bytes as u64))
            .unwrap_or(spec.object_bytes);
    let spec = FixtureSpec {
        groups,
        group_size,
        object_bytes,
    };
    let dead_share = env_u64("GC_BENCH_DEAD_SHARE").unwrap_or(88).min(999);

    if options.dry_run {
        let host = host_info(
            &std::env::temp_dir(),
            &HostOverrides {
                class: options.host_class.clone(),
                nominal_read_mbs: options.nominal_read_mbs,
                nominal_write_mbs: options.nominal_write_mbs,
            },
        );
        let plan = format!(
            "{{\n  \"schema\": \"{SCHEMA}\",\n  \"notRun\": true,\n  \
             \"dryRun\": true,\n  \"scale\": \"{}\",\n  \"ownerOnly\": {},\n  \
             \"ownerApproved\": {},\n  \"runs\": {},\n  \
             \"fixture\": {{\n    \"groups\": {},\n    \"groupSize\": {},\n    \
             \"objectBytes\": {},\n    \"liveObjects\": {},\n    \
             \"livePayloadBytes\": {},\n    \"diskHeadroomBytes\": {}\n  }},\n  \
             \"host\": {},\n  \"command\": {}\n}}",
            scale.label(),
            scale == Scale::Final,
            options.owner_approved,
            runs,
            spec.groups,
            spec.group_size,
            spec.object_bytes,
            spec.live_objects(),
            spec.live_payload_bytes(),
            spec.disk_headroom_bytes(),
            json_host(&host),
            json_string(&std::env::args().collect::<Vec<_>>().join(" ")),
        );
        emit(&plan, &options.out)?;
        eprintln!(
            "gc_bench: dry run for scale {}; nothing was built or measured",
            scale.label()
        );
        return Ok(());
    }

    let tmp_root = options.tmp.clone();
    let scratch = scratch_dir(scale.label(), tmp_root.as_deref(), options.keep)?;
    let available = available_bytes(&scratch);
    let required = if scale == Scale::Final {
        spec.disk_headroom_bytes().max(155_000_000_000)
    } else {
        spec.disk_headroom_bytes()
    };
    if available < required {
        cleanup(&scratch, options.keep);
        return Err(refusal(format!(
            "not enough free space at {}: {} bytes available, {} required (the rebuild's \
             receipt bound is ~2.90x the old file; the runbook asks for >=3x)",
            scratch.display(),
            available,
            required
        )));
    }

    let host = host_info(
        &scratch,
        &HostOverrides {
            class: options.host_class.clone(),
            nominal_read_mbs: options.nominal_read_mbs,
            nominal_write_mbs: options.nominal_write_mbs,
        },
    );
    let store = scratch.join("store.sqlite");
    eprintln!(
        "gc_bench: scale={} groups={} group={} objectBytes={} runs={} tmp={} store={}",
        scale.label(),
        spec.groups,
        spec.group_size,
        spec.object_bytes,
        runs,
        scratch.display(),
        store.display()
    );
    eprintln!(
        "gc_bench: host={} media={} tmpfs={} free={:.1} GB",
        host.host_class,
        host.storage_media,
        host.tmp_filesystem,
        available as f64 / GB
    );

    let (build_seconds, built) = gc_fixture::build_live(&store, spec);
    let dead_target = dead_share;
    let dead =
        gc_fixture::ensure_dead_share(&store, dead_target, 0, 1_000_000_000, spec.object_bytes);
    let after_dead = gc_fixture::pack_stats(&store);
    let physical_after_dead = gc_fixture::path_bytes(&store);
    eprintln!(
        "gc_bench: fixture live={:.3} GB in {:.1} s ({:.1} MB/s); dead +{} objects -> \
         {:.3} GB dead of {:.3} GB ({:.3}%)",
        spec.live_payload_bytes() as f64 / GB,
        build_seconds,
        spec.live_payload_bytes() as f64 / 1e6 / build_seconds.max(1e-9),
        dead.added_objects,
        dead.dead_bytes as f64 / GB,
        after_dead.total as f64 / GB,
        dead.dead_bytes as f64 * 100.0 / after_dead.total.max(1) as f64
    );

    let mut none_runs = Vec::new();
    for run in 0..runs {
        let measured = measure(&store, 90u8.wrapping_add(run as u8), 0, options.cold_cache)?;
        if measured.path != "none" {
            cleanup(&scratch, options.keep);
            return Err(refusal(format!(
                "{:.3}% dead took the {} path, not none; the {} permille setup is wrong",
                dead.dead_bytes as f64 * 100.0 / after_dead.total.max(1) as f64,
                measured.path,
                dead_target
            )));
        }
        eprintln!(
            "gc_bench: none run {}: {:.3} s, {:.2} s/GB live, peakTemp {} B, write {} B",
            run + 1,
            measured.wall_micros as f64 / 1e6,
            seconds_per_micros_to_gb(measured.wall_micros, measured.live_bytes()),
            measured.peak_temp_bytes,
            measured.io.write_bytes
        );
        none_runs.push(measured);
    }

    let mut rebuild_runs = Vec::new();
    let mut dead_bytes = none_runs
        .last()
        .map(|run| run.dead_bytes_bound)
        .unwrap_or(0);
    let mut dead_seed = 2_000_000_000u64;
    for run in 0..runs {
        let added =
            gc_fixture::ensure_dead_share(&store, 100, dead_bytes, dead_seed, spec.object_bytes);
        dead_seed = dead_seed.saturating_add(added.added_objects);
        let measured = measure(
            &store,
            100u8.wrapping_add(run as u8),
            added.added_objects,
            options.cold_cache,
        )?;
        if measured.path != "rebuild" {
            cleanup(&scratch, options.keep);
            return Err(refusal(format!(
                "10% dead took the {} path, not rebuild",
                measured.path
            )));
        }
        eprintln!(
            "gc_bench: rebuild run {}: {:.3} s, {:.2} s/GB live, build {:.2} s/GB new, \
             peakTemp {} B, write {} B, freed {} B, copy/freed {:.2}",
            run + 1,
            measured.wall_micros as f64 / 1e6,
            seconds_per_micros_to_gb(measured.wall_micros, measured.live_bytes()),
            seconds_per_nanos_to_gb(measured.phase_nanos("build"), measured.new_file_bytes),
            measured.peak_temp_bytes,
            measured.io.write_bytes,
            measured.physical_shrink,
            measured.bytes_copied_per_freed_byte()
        );
        dead_bytes = 0;
        rebuild_runs.push(measured);
    }

    let mut cache_drops = 0u64;
    let mut cache_refused = 0u64;
    let mut cache_note = String::new();
    for run in none_runs.iter().chain(rebuild_runs.iter()) {
        if run.cache.dropped {
            cache_drops += 1;
        } else if options.cold_cache {
            cache_refused += 1;
            if cache_note.is_empty() {
                cache_note = run.cache.note.clone();
            }
        }
    }
    if !options.cold_cache {
        cache_note = "warm: --cold-cache not requested".to_owned();
    }
    let cache_state = if !options.cold_cache {
        "warm"
    } else if cache_refused == 0 {
        "cold"
    } else {
        "warm (drop_caches refused)"
    };

    let mut out = String::new();
    let generated = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0);
    let _ = writeln!(
        out,
        "{{\n  \"schema\": \"{SCHEMA}\",\n  \"generatedAtUnixMs\": {generated},\n  \
         \"notRun\": false,\n  \"scale\": \"{}\",\n  \"ownerOnly\": {},\n  \
         \"runsPerArm\": {},\n  \"profile\": \"SqlitePersistenceProfile::Disposable (WAL/OFF)\",\n  \
         \"host\": {},\n  \"cache\": {{\n    \"requested\": {},\n    \"dropsSucceeded\": {},\n    \
         \"dropsRefused\": {},\n    \"state\": {},\n    \"note\": {}\n  }},\n  \
         \"fixture\": {{\n    \"groups\": {},\n    \"groupSize\": {},\n    \
         \"objectBytes\": {},\n    \"liveObjects\": {},\n    \"livePayloadBytes\": {},\n    \
         \"packBytesAfterLive\": {},\n    \"packBytes\": {},\n    \"physicalBytes\": {},\n    \
         \"packLengthMin\": {},\n    \"packLengthMax\": {},\n    \
         \"deadBytes\": {},\n    \"deadFractionPermille\": {},\n    \
         \"fixtureBuildSeconds\": {:.3},\n    \"fixtureBuildMBs\": {:.1},\n    \
         \"deadAddSeconds\": {:.3},\n    \"addedDeadObjects\": {}\n  }},\n  \
         \"arms\": {{{}\n}}\n}}",
        scale.label(),
        scale == Scale::Final,
        runs,
        json_host(&host),
        options.cold_cache,
        cache_drops,
        cache_refused,
        json_string(cache_state),
        json_string(&cache_note),
        spec.groups,
        spec.group_size,
        spec.object_bytes,
        spec.live_objects(),
        spec.live_payload_bytes(),
        built.total,
        after_dead.total,
        physical_after_dead,
        after_dead.min,
        after_dead.max,
        dead.dead_bytes,
        dead.dead_bytes.saturating_mul(1000) / after_dead.total.max(1),
        build_seconds,
        spec.live_payload_bytes() as f64 / 1e6 / build_seconds.max(1e-9),
        dead.seconds,
        dead.added_objects,
        format!(
            "{},\n  {}",
            json_arm("none", &none_runs),
            json_arm("rebuild", &rebuild_runs)
        ),
    );

    emit(&out, &options.out)?;
    cleanup(&scratch, options.keep);
    Ok(())
}
