//! Externally supervised, filesystem-backed C3 comparison runner.
//!
//! The runner deliberately has no in-process candidate registry, fallback, or
//! worker threads. A supervisor starts one directly killable child per sample.
//! Each child performs exactly one complete operation in a fresh FsCas root and
//! writes one bounded record. Missing, duplicate, oversized, timed-out, or
//! malformed records are failures.

use std::collections::HashSet;
use std::env;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitCode, Stdio};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use layerfs_storage::c3::{
    run_c3_create_v1, C3OperationBuffersV1, C3SourceSupplierV1, FileChunkReferenceSpoolV1,
    FilePackIndexSpoolV1,
};
use layerfs_storage::cdc::algorithms::C3CdcAlgorithmV1;
use layerfs_storage::cdc::{
    BorrowedChunkV1, BoundaryConsumerV1, CdcBoundaryConsumerErrorV1, CdcControlV1, ChunkBoundaryV1,
    MAXIMUM_CHUNK_BYTES,
};
use layerfs_storage::content::{ContentSourceErrorV1, ContentSourceV1};
use layerfs_storage::fscas::{FsCasControlV1, FsCasV1};
use layerfs_storage::identity::COMPARISON_WINDOW_BYTES;
use layerfs_storage::limits::{OperationCountersV1, ResourceLedgerV1, MEMORY_PROFILE_32_MIB};
use layerfs_storage::tree::{TreePageSummaryV1, MAX_TREE_OBJECT_BYTES, MAX_TREE_PAGE_SUMMARIES};
use layerfs_storage::CoreResult;

const SCHEMA: &str = "layerfs-c3-qualification-v1";
const REGISTRY_BYTES: &[u8] = include_bytes!("../../tests/fixtures/c3-registry-v1.tsv");
const REGISTRY_ROWS: usize = 204;
const REGISTRY_SHA256: &str = "db8d1f2239cdbcfc3b37a050859533dea547b5d690dc17fd09099a0f6539ea61";
const SMOKE_SHA256: &str = "ea561fb5cdd30a2cd37481e7911cc0df98a473c8ebddcdf32a271748a2a6a025";
const SMOKE_IDS: [&str; 16] = [
    "SMK.01.source_null.prng_1m",
    "SMK.02.fastcdc.primary_vector",
    "SMK.03.seqcdc.slope_jump_vector",
    "SMK.04.engine.prng_1m.contiguous",
    "SMK.05.engine.prng_1m.fragment_4k",
    "SMK.06.engine.prng_1m.forced_wrap",
    "SMK.07.dual_hash.prng_1m",
    "SMK.09.create.prng_8m.of",
    "SMK.10.create.prng_8m.os",
    "SMK.12.update.middle_replace_4k.of",
    "SMK.13.update.middle_replace_4k.os",
    "SMK.14.cas.equal_occupant",
    "SMK.15.cow.middle_replace",
    "SMK.16.ledger_32m.exact",
    "SMK.17.ledger_32m.one_over",
    "SMK.18.fscas.discard_cleanup",
];
const MAX_RESULT_BYTES: u64 = 16 * 1024;
const MAX_ARTIFACT_BYTES: usize = 4 * 1024 * 1024;
const CASE_WALL: Duration = Duration::from_secs(12);
const STREAM_CASE_WALL: Duration = Duration::from_secs(3);
const POLL: Duration = Duration::from_millis(5);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Candidate {
    Of,
    Os,
}

impl Candidate {
    fn parse(value: &str) -> Option<Self> {
        match value {
            "OF" => Some(Self::Of),
            "OS" => Some(Self::Os),
            _ => None,
        }
    }

    const fn name(self) -> &'static str {
        match self {
            Self::Of => "OF",
            Self::Os => "OS",
        }
    }

    const fn implementation(self) -> C3CdcAlgorithmV1 {
        match self {
            Self::Of => C3CdcAlgorithmV1::FastCdc,
            Self::Os => C3CdcAlgorithmV1::SeqCdc,
        }
    }

    const fn index(self) -> usize {
        match self {
            Self::Of => 0,
            Self::Os => 1,
        }
    }
}

#[derive(Clone, Copy)]
enum Pattern {
    Prng,
    Repeated,
}

impl Pattern {
    fn parse(value: &str) -> Option<Self> {
        match value {
            "prng" => Some(Self::Prng),
            "repeated" => Some(Self::Repeated),
            _ => None,
        }
    }

    const fn name(self) -> &'static str {
        match self {
            Self::Prng => "prng",
            Self::Repeated => "repeated",
        }
    }
}

struct PatternSource {
    remaining: u64,
    offset: u64,
    state: u64,
    pattern: Pattern,
    maximum_read: usize,
}

impl ContentSourceV1 for PatternSource {
    fn resident_memory_bound_bytes(&self) -> CoreResult<u64> {
        Ok(core::mem::size_of::<Self>() as u64)
    }

    fn read(&mut self, destination: &mut [u8]) -> Result<usize, ContentSourceErrorV1> {
        let take = destination
            .len()
            .min(self.maximum_read)
            .min(usize::try_from(self.remaining).unwrap_or(usize::MAX));
        for byte in &mut destination[..take] {
            *byte = match self.pattern {
                Pattern::Prng => {
                    self.state ^= self.state << 13;
                    self.state ^= self.state >> 7;
                    self.state ^= self.state << 17;
                    (self.state as u8) ^ (self.offset as u8).wrapping_mul(17)
                }
                Pattern::Repeated => b"layerfs-c3-repeat"[self.offset as usize % 17],
            };
            self.offset += 1;
        }
        self.remaining -= take as u64;
        Ok(take)
    }
}

struct Supplier {
    bytes: u64,
    pattern: Pattern,
    maximum_read: usize,
    seed: u64,
}

impl C3SourceSupplierV1 for Supplier {
    type Source = PatternSource;

    fn resident_memory_bound_bytes(&self) -> CoreResult<u64> {
        Ok(core::mem::size_of::<PatternSource>() as u64)
    }

    fn supply(self) -> CoreResult<Self::Source> {
        Ok(PatternSource {
            remaining: self.bytes,
            offset: 0,
            state: self.seed,
            pattern: self.pattern,
            maximum_read: self.maximum_read,
        })
    }
}

#[derive(Default)]
struct ContinueControl;

impl CdcControlV1 for ContinueControl {
    fn cancellation_requested(&mut self) -> bool {
        false
    }

    fn deadline_exceeded(&mut self) -> bool {
        false
    }
}

impl FsCasControlV1 for ContinueControl {
    fn cancellation_requested(&mut self) -> bool {
        false
    }

    fn deadline_exceeded(&mut self) -> bool {
        false
    }
}

struct TemporaryRoot(PathBuf);

impl TemporaryRoot {
    fn new(label: &str) -> io::Result<Self> {
        let parent = fs::canonicalize(env::temp_dir())?;
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(io::Error::other)?
            .as_nanos();
        Ok(Self(parent.join(format!(
            "layerfs-c3-qualification-{label}-{}-{stamp:032x}",
            std::process::id()
        ))))
    }
}

impl Drop for TemporaryRoot {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[derive(Clone, Copy)]
struct ProcessCpu(Option<u64>);

impl ProcessCpu {
    fn now() -> Self {
        Self(process_cpu_nanos())
    }

    fn elapsed_since(self, before: Self) -> Option<u64> {
        self.0?.checked_sub(before.0?)
    }
}

#[cfg(unix)]
fn process_cpu_nanos() -> Option<u64> {
    let mut value = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: `value` is a valid, writable timespec and the POSIX process CPU
    // clock has no pointer lifetime beyond this call.
    if unsafe { libc::clock_gettime(libc::CLOCK_PROCESS_CPUTIME_ID, &mut value) } != 0 {
        return None;
    }
    let seconds = u64::try_from(value.tv_sec).ok()?;
    let nanos = u64::try_from(value.tv_nsec).ok()?;
    seconds.checked_mul(1_000_000_000)?.checked_add(nanos)
}

#[cfg(not(unix))]
fn process_cpu_nanos() -> Option<u64> {
    None
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("c3-qualification: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let arguments: Vec<String> = env::args().collect();
    match arguments.get(1).map(String::as_str) {
        Some("--sample") => sample_mode(&arguments[2..]),
        Some("--measure") => supervisor_mode(&arguments[2..]),
        Some("--stream-sample") => stream_sample_mode(&arguments[2..]),
        Some("--anchor") => anchor_supervisor_mode(&arguments[2..]),
        Some("--wall") => registry_wall_mode(&arguments[2..]),
        Some("--registry-row") => registry_row_mode(&arguments[2..]),
        Some("--validate-registry") => {
            let registry = validate_registry()?;
            println!(
                "registry rows={} sha256={} exact79={} smoke_sha256={}",
                registry.rows, REGISTRY_SHA256, registry.exact79, SMOKE_SHA256
            );
            Ok(())
        }
        Some("--write-artifact") => writer_mode(&arguments[2..]),
        _ => Err("usage: c3-qualification --measure <output.jsonl> [samples] [bytes] [prng|repeated] | --anchor <output.jsonl> [bytes] | --wall <smoke|qualification> <output.jsonl>".into()),
    }
}

struct RegistrySummary {
    rows: usize,
    exact79: usize,
}

struct RegistryRow<'a> {
    assertion_id: &'a str,
    owning_case_id: &'a str,
}

fn registry_rows() -> Result<Vec<RegistryRow<'static>>, String> {
    let text = core::str::from_utf8(REGISTRY_BYTES).map_err(|error| error.to_string())?;
    let mut rows = Vec::new();
    for line in text.lines().filter(|line| !line.starts_with('#')) {
        let mut columns = line.split('\t');
        let assertion_id = columns.next().ok_or("registry row missing ID")?;
        let owning_case_id = columns.next().ok_or("registry row missing case")?;
        rows.push(RegistryRow {
            assertion_id,
            owning_case_id,
        });
    }
    Ok(rows)
}

fn validate_registry() -> Result<RegistrySummary, String> {
    let actual_hash = sha256_hex(REGISTRY_BYTES);
    if actual_hash != REGISTRY_SHA256 {
        return Err(format!(
            "registry SHA-256 mismatch: expected {REGISTRY_SHA256}, got {actual_hash}"
        ));
    }
    let text = core::str::from_utf8(REGISTRY_BYTES).map_err(|error| error.to_string())?;
    if !text.ends_with('\n') {
        return Err("registry is not LF terminated".into());
    }
    let mut identifiers = HashSet::new();
    let mut ordered = Vec::new();
    let mut exact79 = 0_usize;
    for line in text.lines().filter(|line| !line.starts_with('#')) {
        if line.is_empty() {
            return Err("registry contains an empty assertion row".into());
        }
        let columns: Vec<&str> = line.split('\t').collect();
        if columns.len() != 9 || columns.iter().any(|column| column.is_empty()) {
            return Err(format!("malformed registry row: {line}"));
        }
        if !identifiers.insert(columns[0]) {
            return Err(format!("duplicate registry ID: {}", columns[0]));
        }
        ordered.push(columns[0]);
        exact79 += usize::from(columns[0].starts_with("C30.01"));
    }
    if ordered.len() != REGISTRY_ROWS || exact79 != 79 {
        return Err(format!(
            "registry dimensions mismatch: rows={}, exact79={exact79}",
            ordered.len()
        ));
    }
    let mut smoke_preimage = String::new();
    for identifier in SMOKE_IDS {
        if !identifiers.contains(identifier) {
            return Err(format!("registry missing smoke ID {identifier}"));
        }
        smoke_preimage.push_str(identifier);
        smoke_preimage.push('\n');
    }
    let smoke_hash = sha256_hex(smoke_preimage.as_bytes());
    if smoke_hash != SMOKE_SHA256 {
        return Err(format!("smoke SHA-256 mismatch: {smoke_hash}"));
    }
    Ok(RegistrySummary {
        rows: ordered.len(),
        exact79,
    })
}

fn value_after<'a>(arguments: &'a [String], flag: &str) -> Result<&'a str, String> {
    let position = arguments
        .iter()
        .position(|value| value == flag)
        .ok_or_else(|| format!("missing {flag}"))?;
    arguments
        .get(position + 1)
        .map(String::as_str)
        .ok_or_else(|| format!("missing value after {flag}"))
}

fn sample_mode(arguments: &[String]) -> Result<(), String> {
    let candidate = Candidate::parse(value_after(arguments, "--candidate")?)
        .ok_or_else(|| "invalid candidate".to_string())?;
    let pattern = Pattern::parse(value_after(arguments, "--pattern")?)
        .ok_or_else(|| "invalid pattern".to_string())?;
    let bytes = value_after(arguments, "--bytes")?
        .parse::<u64>()
        .map_err(|error| error.to_string())?;
    let sample = value_after(arguments, "--index")?
        .parse::<u32>()
        .map_err(|error| error.to_string())?;
    let result = PathBuf::from(value_after(arguments, "--result")?);
    let record = run_complete_sample(candidate, pattern, bytes, sample)?;
    if record.len() as u64 > MAX_RESULT_BYTES {
        return Err("sample record exceeds bounded result size".into());
    }
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&result)
        .map_err(|error| error.to_string())?;
    output
        .write_all(record.as_bytes())
        .and_then(|()| output.write_all(b"\n"))
        .map_err(|error| error.to_string())
}

fn registry_row_mode(arguments: &[String]) -> Result<(), String> {
    validate_registry()?;
    let identifier = value_after(arguments, "--id")?;
    let result = PathBuf::from(value_after(arguments, "--result")?);
    let rows = registry_rows()?;
    let row = rows
        .iter()
        .find(|row| row.assertion_id == identifier)
        .ok_or_else(|| format!("unknown registry ID {identifier}"))?;
    let (status, reason) = match identifier {
        "R00.01.registry" => (
            "pass",
            "embedded count, order, uniqueness, and SHA-256 verified",
        ),
        "SMK.09.create.prng_8m.of" => {
            run_complete_sample(Candidate::Of, Pattern::Prng, 8 * 1024 * 1024, 0)?;
            ("pass", "fresh FsCas complete-C3 child passed")
        }
        "SMK.10.create.prng_8m.os" => {
            run_complete_sample(Candidate::Os, Pattern::Prng, 8 * 1024 * 1024, 0)?;
            ("pass", "fresh FsCas complete-C3 child passed")
        }
        "SMK.13.update.middle_replace_4k.os" => (
            "fail",
            "covered by the all-feature Update suite, not duplicated by this registry-row executable",
        ),
        "SMK.15.cow.middle_replace" => (
            "fail",
            "covered by the all-feature exact-locality suite, not duplicated by this registry-row executable",
        ),
        _ => (
            "fail",
            "assertion has no independently executable pinned registry-row implementation",
        ),
    };
    let record = format!(
        "{{\"schema\":\"{SCHEMA}\",\"record_type\":\"registry-terminal\",\"assertion_id\":\"{}\",\"owning_case_id\":\"{}\",\"status\":\"{}\",\"reason\":\"{}\"}}\n",
        row.assertion_id, row.owning_case_id, status, reason
    );
    if record.len() as u64 > MAX_RESULT_BYTES {
        return Err("registry terminal exceeds result bound".into());
    }
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(result)
        .map_err(|error| error.to_string())?;
    output
        .write_all(record.as_bytes())
        .map_err(|error| error.to_string())
}

fn registry_wall_mode(arguments: &[String]) -> Result<(), String> {
    let registry = validate_registry()?;
    let wall = arguments.first().ok_or("missing wall name")?;
    if wall != "smoke" && wall != "qualification" {
        return Err("wall must be smoke or qualification".into());
    }
    let artifact = PathBuf::from(arguments.get(1).ok_or("missing artifact path")?);
    let rows = registry_rows()?;
    let selected: Vec<&RegistryRow<'_>> = if wall == "smoke" {
        SMOKE_IDS
            .iter()
            .map(|identifier| {
                rows.iter()
                    .find(|row| row.assertion_id == *identifier)
                    .ok_or_else(|| format!("missing smoke registry ID {identifier}"))
            })
            .collect::<Result<_, _>>()?
    } else {
        rows.iter().collect()
    };
    let temporary = TemporaryRoot::new(wall).map_err(|error| error.to_string())?;
    fs::create_dir(&temporary.0).map_err(|error| error.to_string())?;
    let executable = env::current_exe().map_err(|error| error.to_string())?;
    let started = Instant::now();
    let mut artifact_contents = String::new();
    let mut passed = 0_usize;
    let mut failed = 0_usize;
    let mut seen = HashSet::new();
    for (ordinal, row) in selected.iter().enumerate() {
        if !seen.insert(row.assertion_id) {
            return Err(format!(
                "duplicate scheduled registry ID {}",
                row.assertion_id
            ));
        }
        let result = temporary.0.join(format!("{ordinal:03}.json"));
        let stderr_path = temporary.0.join(format!("{ordinal:03}.stderr"));
        let stderr = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&stderr_path)
            .map_err(|error| error.to_string())?;
        let child = Command::new(&executable)
            .args([
                "--registry-row",
                "--id",
                row.assertion_id,
                "--result",
                result.to_str().ok_or("non-UTF-8 result path")?,
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::from(stderr))
            .spawn()
            .map_err(|error| error.to_string())?;
        wait_direct_child(child, CASE_WALL).map_err(|error| {
            let detail = fs::read_to_string(&stderr_path).unwrap_or_default();
            format!("registry row {}: {error}; {detail}", row.assertion_id)
        })?;
        let metadata = fs::metadata(&result).map_err(|error| error.to_string())?;
        if metadata.len() == 0 || metadata.len() > MAX_RESULT_BYTES {
            return Err(format!(
                "invalid registry terminal for {}",
                row.assertion_id
            ));
        }
        let record = fs::read_to_string(&result).map_err(|error| error.to_string())?;
        if !record.ends_with('\n') || !record.contains("\"record_type\":\"registry-terminal\"") {
            return Err(format!(
                "partial registry terminal for {}",
                row.assertion_id
            ));
        }
        if record.contains("\"status\":\"pass\"") {
            passed += 1;
        } else if record.contains("\"status\":\"fail\"") {
            failed += 1;
        } else {
            return Err(format!(
                "non-terminal registry state for {}",
                row.assertion_id
            ));
        }
        artifact_contents.push_str(&record);
    }
    if passed + failed != selected.len() {
        return Err("registry wall terminal count mismatch".into());
    }
    artifact_contents.push_str(&format!(
        "{{\"schema\":\"{SCHEMA}\",\"record_type\":\"registry-wall\",\"wall\":\"{}\",\"status\":\"{}\",\"scheduled\":{},\"terminal\":{},\"passed\":{},\"failed\":{},\"registry_rows\":{},\"registry_sha256\":\"{}\",\"smoke_sha256\":\"{}\",\"suite_wall_ns\":{},\"supervision\":\"one-direct-child-per-registry-row\",\"scope\":\"preselection-assertion-audit\",\"selection\":\"FastCDC/OF\"}}\n",
        wall,
        if failed == 0 { "pass" } else { "fail" },
        selected.len(),
        passed + failed,
        passed,
        failed,
        registry.rows,
        REGISTRY_SHA256,
        SMOKE_SHA256,
        started.elapsed().as_nanos(),
    ));
    if artifact_contents.len() > MAX_ARTIFACT_BYTES {
        return Err("registry wall artifact exceeds bound".into());
    }
    write_artifact_bounded(&executable, &artifact, &artifact_contents)
}

fn run_complete_sample(
    candidate: Candidate,
    pattern: Pattern,
    bytes: u64,
    sample: u32,
) -> Result<String, String> {
    let fixture = TemporaryRoot::new(candidate.name()).map_err(|error| error.to_string())?;
    let cas = FsCasV1::create_new(&fixture.0).map_err(|error| format!("{error:?}"))?;
    let references_path = fixture.0.join("reference-spool");
    let metadata_path = fixture.0.join("metadata-spool");
    let mut references = FileChunkReferenceSpoolV1::create(&references_path)
        .map_err(|error| format!("{error:?}"))?;
    let mut metadata =
        FilePackIndexSpoolV1::create(&metadata_path).map_err(|error| format!("{error:?}"))?;
    let ledger = ResourceLedgerV1::new(MEMORY_PROFILE_32_MIB);
    let mut counters = OperationCountersV1::default();
    let mut source_window = vec![0_u8; MAXIMUM_CHUNK_BYTES];
    let mut cdc_ring = vec![0_u8; MAXIMUM_CHUNK_BYTES];
    let mut incoming = vec![0_u8; COMPARISON_WINDOW_BYTES];
    let mut occupied = vec![0_u8; COMPARISON_WINDOW_BYTES];
    let mut tree_object = vec![0_u8; MAX_TREE_OBJECT_BYTES];
    let mut tree_pages = vec![None::<TreePageSummaryV1>; MAX_TREE_PAGE_SUMMARIES];
    let mut traversal = vec![0_u8; 64 * 1024];
    let mut control = ContinueControl;
    let before_cpu = ProcessCpu::now();
    let before_wall = Instant::now();
    let handoff = run_c3_create_v1(
        &cas,
        candidate.implementation(),
        b"payload.bin",
        0o644,
        bytes,
        Supplier {
            bytes,
            pattern,
            maximum_read: 4096,
            seed: 0x9e37_79b9_7f4a_7c15,
        },
        &mut references,
        &mut metadata,
        C3OperationBuffersV1 {
            source: source_window
                .as_mut_slice()
                .try_into()
                .map_err(|_| "source window shape")?,
            cdc_ring: cdc_ring
                .as_mut_slice()
                .try_into()
                .map_err(|_| "ring shape")?,
            incoming_comparison: incoming
                .as_mut_slice()
                .try_into()
                .map_err(|_| "incoming shape")?,
            occupied_comparison: occupied
                .as_mut_slice()
                .try_into()
                .map_err(|_| "occupied shape")?,
            tree_object: tree_object
                .as_mut_slice()
                .try_into()
                .map_err(|_| "tree object shape")?,
            tree_pages: &mut tree_pages,
            traversal_state: &mut traversal,
        },
        &mut control,
        &ledger,
        &mut counters,
    )
    .map_err(|error| format!("complete C3 failed: {error:?}; counters={counters:?}"))?;
    let wall_ns = before_wall.elapsed().as_nanos();
    let cpu_ns = ProcessCpu::now().elapsed_since(before_cpu);
    let preparation_entries = fs::read_dir(fixture.0.join("preparation"))
        .map_err(|error| error.to_string())?
        .count();
    let reference_after = fs::metadata(&references_path)
        .map_err(|error| error.to_string())?
        .len();
    let metadata_after = fs::metadata(&metadata_path)
        .map_err(|error| error.to_string())?
        .len();
    if ledger.admitted_slots() != 0
        || preparation_entries != 0
        || reference_after != 0
        || metadata_after != 0
        || !counters.has_zero_forbidden_work()
        || counters.closure_fences != 1
    {
        return Err("post-return resource or forbidden-work invariant failed".into());
    }
    Ok(format!(
        "{{\"schema\":\"{SCHEMA}\",\"status\":\"pass\",\"candidate\":\"{}\",\"pattern\":\"{}\",\"sample\":{},\"logical_bytes\":{},\"wall_ns\":{},\"cpu_ns\":{},\"pack_bytes\":{},\"object_count\":{},\"source_read_calls\":{},\"source_bytes_read\":{},\"fscas_read_calls\":{},\"fscas_read_bytes\":{},\"fscas_write_bytes\":{},\"physical_created\":{},\"physical_reused\":{},\"tree_created\":{},\"tree_reused\":{},\"reference_spool_peak\":{},\"index_spool_peak\":{},\"temporary_preparation_peak\":{},\"exact_one_slot_ledger_ceiling\":{},\"planned_logical_allocation_high_water\":{},\"ledger_slots_after\":{},\"preparation_entries_after\":{},\"reference_bytes_after\":{},\"index_bytes_after\":{},\"unreachable_residue_bytes\":{},\"fallback_attempts\":{},\"redispatches\":{},\"provider_switches\":{},\"cdc_switches\":{},\"publication_dispatches\":{},\"file_sync_calls\":{},\"directory_sync_calls\":{},\"allocator_high_water\":null,\"rss_high_water\":null,\"open_descriptors_after\":null}}",
        candidate.name(),
        pattern.name(),
        sample,
        bytes,
        wall_ns,
        cpu_ns.map_or_else(|| "null".to_string(), |value| value.to_string()),
        handoff.pack().pack_len(),
        handoff.object_count(),
        counters.source_read_calls,
        counters.source_bytes_read,
        counters.fscas_read_calls,
        counters.fscas_bytes_read,
        counters.fscas_bytes_written,
        counters.physical_objects_created,
        counters.physical_objects_reused,
        counters.tree_nodes_created,
        counters.tree_nodes_reused,
        handoff.reference_spool_bytes().unwrap_or(0),
        handoff.index_spool_bytes().unwrap_or(0),
        counters.temporary_preparation_bytes,
        ledger.high_water_bytes(),
        ledger.planned_high_water_bytes(),
        ledger.admitted_slots(),
        preparation_entries,
        reference_after,
        metadata_after,
        counters.unreachable_installed_residue_bytes,
        counters.fallback_attempts,
        counters.retries_or_redispatches,
        counters.provider_switches,
        counters.cdc_switches,
        counters.publication_dispatches,
        counters.file_sync_calls,
        counters.directory_sync_calls,
    ))
}

#[derive(Default)]
struct CountingConsumer {
    chunks: u64,
    bytes: u64,
}

impl BoundaryConsumerV1 for CountingConsumer {
    fn accept(
        &mut self,
        boundary: ChunkBoundaryV1,
        chunk: BorrowedChunkV1<'_>,
    ) -> Result<(), CdcBoundaryConsumerErrorV1> {
        if boundary.len() != chunk.len() as u64 {
            return Err(CdcBoundaryConsumerErrorV1::Refused);
        }
        self.chunks = self
            .chunks
            .checked_add(1)
            .ok_or(CdcBoundaryConsumerErrorV1::Refused)?;
        self.bytes = self
            .bytes
            .checked_add(chunk.len() as u64)
            .ok_or(CdcBoundaryConsumerErrorV1::Refused)?;
        Ok(())
    }
}

fn stream_sample_mode(arguments: &[String]) -> Result<(), String> {
    let candidate = Candidate::parse(value_after(arguments, "--candidate")?)
        .ok_or_else(|| "invalid candidate".to_string())?;
    let bytes = value_after(arguments, "--bytes")?
        .parse::<u64>()
        .map_err(|error| error.to_string())?;
    let sample = value_after(arguments, "--index")?
        .parse::<u32>()
        .map_err(|error| error.to_string())?;
    let warmup = value_after(arguments, "--warmup")? == "true";
    let result = PathBuf::from(value_after(arguments, "--result")?);
    let mut source = PatternSource {
        remaining: bytes,
        offset: 0,
        state: 0x9e37_79b9_7f4a_7c15,
        pattern: Pattern::Prng,
        maximum_read: 4096,
    };
    let mut fragment = vec![0_u8; 4096];
    let mut ring = vec![0_u8; MAXIMUM_CHUNK_BYTES];
    let mut control = ContinueControl;
    let mut consumer = CountingConsumer::default();
    let before_cpu = ProcessCpu::now();
    let before_wall = Instant::now();
    let mut stream = candidate
        .implementation()
        .stream(&mut ring, &mut control)
        .map_err(|error| format!("stream construction failed: {error:?}"))?;
    let mut source_read_calls = 0_u64;
    let mut source_bytes = 0_u64;
    loop {
        source_read_calls += 1;
        let read = source
            .read(&mut fragment)
            .map_err(|_| "pattern source failed")?;
        if read == 0 {
            break;
        }
        source_bytes += read as u64;
        stream
            .push(Ok(&fragment[..read]), &mut control, &mut consumer)
            .map_err(|error| format!("stream push failed: {error:?}"))?;
    }
    stream
        .finish(&mut control, &mut consumer)
        .map_err(|error| format!("stream finish failed: {error:?}"))?;
    let wall_ns = before_wall.elapsed().as_nanos();
    let cpu_ns = ProcessCpu::now().elapsed_since(before_cpu);
    let counters = stream.counters();
    if source_bytes != bytes || consumer.bytes != bytes || source.remaining != 0 {
        return Err("streamed byte accounting mismatch".into());
    }
    let record = format!(
        "{{\"schema\":\"{SCHEMA}\",\"record_type\":\"C30.05\",\"status\":\"pass\",\"candidate\":\"{}\",\"sample\":{},\"warmup\":{},\"logical_bytes\":{},\"wall_ns\":{},\"cpu_ns\":{},\"source_read_calls\":{},\"source_bytes_read\":{},\"chunks\":{},\"ring_fills\":{},\"ring_wrap_spans\":{},\"scan_calls\":{},\"scan_bytes\":{},\"boundary_inspected_bytes\":{}}}",
        candidate.name(),
        sample,
        warmup,
        bytes,
        wall_ns,
        cpu_ns.map_or_else(|| "null".to_string(), |value| value.to_string()),
        source_read_calls,
        source_bytes,
        consumer.chunks,
        counters.ring_fills,
        counters.ring_wrap_spans,
        counters.scan_calls,
        counters.scan_bytes,
        counters.boundary_inspected_bytes,
    );
    if record.len() as u64 > MAX_RESULT_BYTES {
        return Err("stream sample record exceeds bound".into());
    }
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(result)
        .map_err(|error| error.to_string())?;
    output
        .write_all(record.as_bytes())
        .and_then(|()| output.write_all(b"\n"))
        .map_err(|error| error.to_string())
}

#[derive(Clone, Copy)]
struct Distribution {
    median: f64,
    mad: f64,
    minimum: f64,
    maximum: f64,
}

impl Distribution {
    fn json(self) -> String {
        let relative_mad = if self.median == 0.0 {
            0.0
        } else {
            self.mad / self.median
        };
        format!(
            "{{\"median\":{:.6},\"mad\":{:.6},\"mad_over_median\":{:.9},\"minimum\":{:.6},\"maximum\":{:.6}}}",
            self.median, self.mad, relative_mad, self.minimum, self.maximum
        )
    }
}

fn distribution(values: &[f64]) -> Result<Distribution, String> {
    if values.is_empty() || values.iter().any(|value| !value.is_finite()) {
        return Err("empty or non-finite measurement population".into());
    }
    let mut ordered = values.to_vec();
    ordered.sort_by(f64::total_cmp);
    let median = median_sorted(&ordered);
    let mut deviations: Vec<f64> = ordered.iter().map(|value| (value - median).abs()).collect();
    deviations.sort_by(f64::total_cmp);
    Ok(Distribution {
        median,
        mad: median_sorted(&deviations),
        minimum: ordered[0],
        maximum: ordered[ordered.len() - 1],
    })
}

fn median_sorted(values: &[f64]) -> f64 {
    let midpoint = values.len() / 2;
    if values.len() % 2 == 0 {
        (values[midpoint - 1] + values[midpoint]) / 2.0
    } else {
        values[midpoint]
    }
}

fn distributions(values: &[Vec<f64>; 2]) -> Result<[Distribution; 2], String> {
    Ok([distribution(&values[0])?, distribution(&values[1])?])
}

fn optional_distributions(values: &[Vec<f64>; 2]) -> Result<[Option<Distribution>; 2], String> {
    let value = |values: &[f64]| {
        if values.is_empty() {
            Ok(None)
        } else {
            distribution(values).map(Some)
        }
    };
    Ok([value(&values[0])?, value(&values[1])?])
}

fn paired_distributions(rounds: &[[Option<u64>; 2]]) -> Result<Distribution, String> {
    let mut ratios = Vec::new();
    for round in rounds {
        let of = round[0].ok_or("missing OF round observation")? as f64;
        let os = round[1].ok_or("missing OS round observation")? as f64;
        ratios.push(os / of);
    }
    distribution(&ratios)
}

fn optional_paired_distributions(
    rounds: &[[Option<u64>; 2]],
) -> Result<Option<Distribution>, String> {
    if rounds.iter().any(|round| round.iter().any(Option::is_none)) {
        return Ok(None);
    }
    paired_distributions(rounds).map(Some)
}

fn named_distributions(values: &[Distribution; 2]) -> String {
    format!(
        "{{\"OF\":{},\"OS\":{}}}",
        values[0].json(),
        values[1].json()
    )
}

fn named_optional_distributions(values: &[Option<Distribution>; 2]) -> String {
    let json =
        |value: Option<Distribution>| value.map_or_else(|| "null".to_string(), Distribution::json);
    format!("{{\"OF\":{},\"OS\":{}}}", json(values[0]), json(values[1]))
}

fn named_ratio(value: Distribution) -> String {
    format!("{{\"OS_over_OF\":{}}}", value.json())
}

fn named_optional_ratio(value: Option<Distribution>) -> String {
    let json = value.map_or_else(|| "null".to_string(), Distribution::json);
    format!("{{\"OS_over_OF\":{json}}}")
}

fn json_optional_u64(record: &str, key: &str) -> Result<Option<u64>, String> {
    let needle = format!("\"{key}\":");
    let value = record
        .split_once(&needle)
        .map(|(_, value)| value)
        .ok_or_else(|| format!("record missing {key}"))?;
    if value.starts_with("null") {
        return Ok(None);
    }
    let digits: String = value.chars().take_while(char::is_ascii_digit).collect();
    if digits.is_empty() {
        return Err(format!("record has malformed {key}"));
    }
    digits
        .parse::<u64>()
        .map(Some)
        .map_err(|error| error.to_string())
}

fn json_u64(record: &str, key: &str) -> Result<u64, String> {
    json_optional_u64(record, key)?.ok_or_else(|| format!("record has unavailable {key}"))
}

fn anchor_supervisor_mode(arguments: &[String]) -> Result<(), String> {
    let registry = validate_registry()?;
    let artifact = PathBuf::from(arguments.first().ok_or("missing artifact path")?);
    let bytes = arguments
        .get(1)
        .map(String::as_str)
        .unwrap_or("67108864")
        .parse::<u64>()
        .map_err(|error| error.to_string())?;
    if bytes == 0 || bytes > 64 * 1024 * 1024 {
        return Err("anchor byte count outside bound".into());
    }
    let temporary = TemporaryRoot::new("anchor-supervisor").map_err(|error| error.to_string())?;
    fs::create_dir(&temporary.0).map_err(|error| error.to_string())?;
    let executable = env::current_exe().map_err(|error| error.to_string())?;
    let suite_start = Instant::now();
    let mut records = Vec::new();
    let mut measured_throughput = [Vec::new(), Vec::new()];
    for candidate in [Candidate::Of, Candidate::Os] {
        for execution in 0..4_u32 {
            let warmup = execution == 0;
            let result = temporary.0.join(format!(
                "{}-{}-{execution}.json",
                candidate.name(),
                if warmup { "warmup" } else { "measured" }
            ));
            let stderr_path = result.with_extension("stderr");
            let stderr = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&stderr_path)
                .map_err(|error| error.to_string())?;
            let child = Command::new(&executable)
                .args([
                    "--stream-sample",
                    "--candidate",
                    candidate.name(),
                    "--bytes",
                    &bytes.to_string(),
                    "--index",
                    &execution.saturating_sub(1).to_string(),
                    "--warmup",
                    if warmup { "true" } else { "false" },
                    "--result",
                    result.to_str().ok_or("non-UTF-8 result path")?,
                ])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::from(stderr))
                .spawn()
                .map_err(|error| error.to_string())?;
            wait_direct_child(child, STREAM_CASE_WALL).map_err(|error| {
                let detail = fs::read_to_string(&stderr_path).unwrap_or_default();
                format!(
                    "{} anchor execution {execution}: {error}; {detail}",
                    candidate.name()
                )
            })?;
            let metadata = fs::metadata(&result).map_err(|error| error.to_string())?;
            if metadata.len() == 0 || metadata.len() > MAX_RESULT_BYTES {
                return Err("anchor result missing, empty, or oversized".into());
            }
            let record = fs::read_to_string(result).map_err(|error| error.to_string())?;
            if !record.ends_with('\n') || !record.contains("\"status\":\"pass\"") {
                return Err("anchor result is partial or failed".into());
            }
            if !warmup {
                let wall_ns = json_u64(&record, "wall_ns")?;
                let throughput =
                    bytes as f64 * 1_000_000_000.0 / wall_ns as f64 / (1024.0 * 1024.0);
                measured_throughput[usize::from(candidate == Candidate::Os)].push(throughput);
            }
            records.push(record);
        }
    }
    let of_throughput = distribution(&measured_throughput[0])?;
    let os_throughput = distribution(&measured_throughput[1])?;
    let mut complete_artifact = String::new();
    for record in records {
        complete_artifact.push_str(&record);
    }
    complete_artifact.push_str(&format!(
        "{{\"schema\":\"{SCHEMA}\",\"record_type\":\"C30.05-suite\",\"status\":\"complete\",\"registry_rows\":{},\"registry_sha256\":\"{}\",\"logical_bytes\":{},\"warmups_per_candidate\":1,\"measured_samples_per_candidate\":3,\"throughput_mib_s\":{{\"OF\":{},\"OS\":{}}},\"gate_mib_s\":300.0,\"of_gate_pass\":{},\"os_gate_pass\":{},\"baseline\":\"FastCDC/OF\",\"suite_wall_ns\":{},\"case_wall_ms\":{},\"supervision\":\"direct-child-no-descendants\"}}\n",
        registry.rows,
        REGISTRY_SHA256,
        bytes,
        of_throughput.json(),
        os_throughput.json(),
        of_throughput.median >= 300.0,
        os_throughput.median >= 300.0,
        suite_start.elapsed().as_nanos(),
        STREAM_CASE_WALL.as_millis(),
    ));
    if complete_artifact.len() > MAX_ARTIFACT_BYTES {
        return Err("anchor artifact exceeds bound".into());
    }
    write_artifact_bounded(&executable, &artifact, &complete_artifact)
}

fn supervisor_mode(arguments: &[String]) -> Result<(), String> {
    let registry = validate_registry()?;
    let artifact = PathBuf::from(arguments.first().ok_or("missing artifact path")?);
    let samples = arguments
        .get(1)
        .map(String::as_str)
        .unwrap_or("15")
        .parse::<u32>()
        .map_err(|error| error.to_string())?;
    let bytes = arguments
        .get(2)
        .map(String::as_str)
        .unwrap_or("8388608")
        .parse::<u64>()
        .map_err(|error| error.to_string())?;
    let pattern = match arguments.get(3) {
        Some(value) => Pattern::parse(value).ok_or_else(|| "invalid pattern".to_string())?,
        None => Pattern::Prng,
    };
    if samples == 0 || samples > 32 || bytes > 64 * 1024 * 1024 {
        return Err("measurement request outside runner bounds".into());
    }
    let temporary = TemporaryRoot::new("supervisor").map_err(|error| error.to_string())?;
    fs::create_dir(&temporary.0).map_err(|error| error.to_string())?;
    let executable = env::current_exe().map_err(|error| error.to_string())?;
    let mut records = Vec::new();
    let mut wall_by_candidate = [Vec::new(), Vec::new()];
    let mut cpu_by_candidate = [Vec::new(), Vec::new()];
    let mut pack_by_candidate = [Vec::new(), Vec::new()];
    let mut wall_by_round = vec![[None; 2]; samples as usize];
    let mut cpu_by_round = vec![[None; 2]; samples as usize];
    let suite_start = Instant::now();
    for round in 0..samples {
        let order = if round % 2 == 0 {
            [Candidate::Of, Candidate::Os]
        } else {
            [Candidate::Os, Candidate::Of]
        };
        for candidate in order {
            let result = temporary
                .0
                .join(format!("{}-{round:02}.json", candidate.name()));
            let stderr_path = temporary
                .0
                .join(format!("{}-{round:02}.stderr", candidate.name()));
            let stderr = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&stderr_path)
                .map_err(|error| error.to_string())?;
            let child = Command::new(&executable)
                .args([
                    "--sample",
                    "--candidate",
                    candidate.name(),
                    "--pattern",
                    pattern.name(),
                    "--bytes",
                    &bytes.to_string(),
                    "--index",
                    &round.to_string(),
                    "--result",
                    result.to_str().ok_or("non-UTF-8 result path")?,
                ])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::from(stderr))
                .spawn()
                .map_err(|error| error.to_string())?;
            wait_direct_child(child, CASE_WALL)?;
            let metadata = fs::metadata(&result).map_err(|error| {
                let detail = fs::read_to_string(&stderr_path).unwrap_or_default();
                format!(
                    "missing result for {} round {round}: {error}; {detail}",
                    candidate.name()
                )
            })?;
            if metadata.len() == 0 || metadata.len() > MAX_RESULT_BYTES {
                return Err(format!(
                    "invalid result size for {} round {round}: {}",
                    candidate.name(),
                    metadata.len()
                ));
            }
            let mut record = String::new();
            File::open(&result)
                .and_then(|mut file| file.read_to_string(&mut record))
                .map_err(|error| error.to_string())?;
            if !record.ends_with('\n') || !record.contains("\"status\":\"pass\"") {
                return Err(format!(
                    "partial or failed result for {} round {round}",
                    candidate.name()
                ));
            }
            let candidate_index = candidate.index();
            let wall_ns = json_u64(&record, "wall_ns")?;
            let cpu_ns = json_optional_u64(&record, "cpu_ns")?;
            wall_by_candidate[candidate_index].push(wall_ns as f64);
            pack_by_candidate[candidate_index].push(json_u64(&record, "pack_bytes")? as f64);
            wall_by_round[round as usize][candidate_index] = Some(wall_ns);
            if let Some(cpu_ns) = cpu_ns {
                cpu_by_candidate[candidate_index].push(cpu_ns as f64);
                cpu_by_round[round as usize][candidate_index] = Some(cpu_ns);
            }
            records.push(record);
        }
    }
    let wall_statistics = distributions(&wall_by_candidate)?;
    let cpu_statistics = optional_distributions(&cpu_by_candidate)?;
    let pack_statistics = distributions(&pack_by_candidate)?;
    let wall_ratios = paired_distributions(&wall_by_round)?;
    let cpu_ratios = optional_paired_distributions(&cpu_by_round)?;
    let summary = format!(
        "{{\"schema\":\"{SCHEMA}\",\"record_type\":\"suite\",\"status\":\"complete\",\"registry_rows\":{},\"registry_sha256\":\"{}\",\"exact_equivalence_rows\":{},\"smoke_sha256\":\"{}\",\"samples_per_candidate\":{},\"logical_bytes\":{},\"pattern\":\"{}\",\"child_count\":{},\"wall_ns\":{},\"cpu_ns\":{},\"pack_bytes\":{},\"paired_wall_ratios\":{},\"paired_cpu_ratios\":{},\"suite_wall_ns\":{},\"supervision\":\"direct-child-no-descendants\",\"case_wall_ms\":{},\"algorithm_profile_binding\":{{\"OF\":true,\"OS\":true}},\"baseline\":\"FastCDC/OF\",\"selection\":\"FastCDC/OF\",\"os_optimization\":\"deferred-to-L1.6\"}}\n",
        registry.rows,
        REGISTRY_SHA256,
        registry.exact79,
        SMOKE_SHA256,
        samples,
        bytes,
        pattern.name(),
        records.len(),
        named_distributions(&wall_statistics),
        named_optional_distributions(&cpu_statistics),
        named_distributions(&pack_statistics),
        named_ratio(wall_ratios),
        named_optional_ratio(cpu_ratios),
        suite_start.elapsed().as_nanos(),
        CASE_WALL.as_millis(),
    );
    let mut complete_artifact = String::new();
    for record in records {
        complete_artifact.push_str(&record);
    }
    complete_artifact.push_str(&summary);
    if complete_artifact.len() > MAX_ARTIFACT_BYTES {
        return Err("artifact exceeds bounded in-memory result size".into());
    }
    write_artifact_bounded(&executable, &artifact, &complete_artifact)
}

fn writer_mode(arguments: &[String]) -> Result<(), String> {
    let artifact = PathBuf::from(arguments.first().ok_or("missing artifact path")?);
    let mut bytes = Vec::new();
    io::stdin()
        .take((MAX_ARTIFACT_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.is_empty() || bytes.len() > MAX_ARTIFACT_BYTES || !bytes.ends_with(b"\n") {
        return Err("writer received missing, oversized, or partial artifact".into());
    }
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&artifact)
        .map_err(|error| error.to_string())?;
    output.write_all(&bytes).map_err(|error| error.to_string())
}

fn write_artifact_bounded(
    executable: &PathBuf,
    artifact: &Path,
    contents: &str,
) -> Result<(), String> {
    let mut child = Command::new(executable)
        .args([
            "--write-artifact",
            artifact.to_str().ok_or("non-UTF-8 artifact path")?,
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|error| error.to_string())?;
    child
        .stdin
        .take()
        .ok_or("writer stdin unavailable")?
        .write_all(contents.as_bytes())
        .map_err(|error| error.to_string())?;
    wait_direct_child(child, Duration::from_secs(2))
}

fn wait_direct_child(mut child: Child, deadline: Duration) -> Result<(), String> {
    let started = Instant::now();
    loop {
        match child.try_wait().map_err(|error| error.to_string())? {
            Some(status) if status.success() => return Ok(()),
            Some(status) => return Err(format!("sample child exited {status}")),
            None if started.elapsed() < deadline => thread::sleep(POLL),
            None => {
                child.kill().map_err(|error| error.to_string())?;
                let _ = child.wait();
                return Err("sample child timeout".into());
            }
        }
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];
    let bit_len = (bytes.len() as u64).wrapping_mul(8);
    let padded_len = (bytes.len() + 9).div_ceil(64) * 64;
    let mut padded = vec![0_u8; padded_len];
    padded[..bytes.len()].copy_from_slice(bytes);
    padded[bytes.len()] = 0x80;
    padded[padded_len - 8..].copy_from_slice(&bit_len.to_be_bytes());
    let mut state = [
        0x6a09e667_u32,
        0xbb67ae85,
        0x3c6ef372,
        0xa54ff53a,
        0x510e527f,
        0x9b05688c,
        0x1f83d9ab,
        0x5be0cd19,
    ];
    for chunk in padded.chunks_exact(64) {
        let mut words = [0_u32; 64];
        for (index, word) in words[..16].iter_mut().enumerate() {
            *word = u32::from_be_bytes(chunk[index * 4..index * 4 + 4].try_into().unwrap());
        }
        for index in 16..64 {
            let s0 = words[index - 15].rotate_right(7)
                ^ words[index - 15].rotate_right(18)
                ^ (words[index - 15] >> 3);
            let s1 = words[index - 2].rotate_right(17)
                ^ words[index - 2].rotate_right(19)
                ^ (words[index - 2] >> 10);
            words[index] = words[index - 16]
                .wrapping_add(s0)
                .wrapping_add(words[index - 7])
                .wrapping_add(s1);
        }
        let mut work = state;
        for index in 0..64 {
            let big1 =
                work[4].rotate_right(6) ^ work[4].rotate_right(11) ^ work[4].rotate_right(25);
            let choose = (work[4] & work[5]) ^ ((!work[4]) & work[6]);
            let first = work[7]
                .wrapping_add(big1)
                .wrapping_add(choose)
                .wrapping_add(K[index])
                .wrapping_add(words[index]);
            let big0 =
                work[0].rotate_right(2) ^ work[0].rotate_right(13) ^ work[0].rotate_right(22);
            let majority = (work[0] & work[1]) ^ (work[0] & work[2]) ^ (work[1] & work[2]);
            let second = big0.wrapping_add(majority);
            work = [
                first.wrapping_add(second),
                work[0],
                work[1],
                work[2],
                work[3].wrapping_add(first),
                work[4],
                work[5],
                work[6],
            ];
        }
        for (value, addition) in state.iter_mut().zip(work) {
            *value = value.wrapping_add(addition);
        }
    }
    let mut output = String::with_capacity(64);
    for word in state {
        use core::fmt::Write as _;
        write!(&mut output, "{word:08x}").unwrap();
    }
    output
}
