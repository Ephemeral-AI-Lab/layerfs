use layerfs_core::cdc::{CdcCounters, FastCdc, MAXIMUM_CHUNK_BYTES};
use layerfs_core::limits::{MAX_CHILD_REFERENCES, MAX_OBJECT_BYTES};
use layerfs_core::object::HEADER_LEN;
use layerfs_core::{
    chunk_id, decode_object, encode_object, CanonicalName, CoreError, DirectoryEntry, Object,
    ObjectId, ObjectKind, ObjectReference,
};
use layerfs_engine::{
    AppendOnlyCounters, AppendOnlyEngine, DeltaRecord, Engine, EngineCounters, EngineError,
    EngineResult, PutOutcome, RootRecord, SqliteProfile,
};
use std::env;
use std::error::Error;
use std::fs::{self, File};
use std::io::{self, BufReader, Read, Seek, SeekFrom};
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::time::Instant;

const HISTORICAL_SOURCE_BYTES: u64 = 100 * 1024 * 1024;
const HISTORICAL_RAW_BLAKE3: &str =
    "0855eedd9498bf31a1eafb5a2f00bf84f646db5153cc86632fcb0cc0e180fb36";
const HISTORICAL_LOGICAL_V1_BLAKE3: &str =
    "52ce153eab81e33a0243a25a47a8805a86ba9bec125a27bee3c50de647cdafbc";
const HISTORICAL_SHA256: &str = "27f82e57f589b7ed79f28a8cef02acd2db82682fbccb35cdd6b48a136d98a7d6";
const EXPECTED_CURRENT_CDC_CHUNKS: u64 = 4_801;
const EXPECTED_SUBMITTED_OBJECTS: u64 = 4_803;
const EXPECTED_CREATED_OBJECTS: u64 = 265;
const EXPECTED_REUSED_OBJECTS: u64 = 4_538;
const SQLITE_CLOSURE_PASSES: u64 = 4;
const SQLITE_DESCENDANTS_PER_PASS: u64 = EXPECTED_CURRENT_CDC_CHUNKS + 1;
const CROSS_BOUNDARY_SIDE_BYTES: usize = 2_048;
const SOURCE_READER_BYTES: usize = 64 * 1024;
const PREFLIGHT_READER_BYTES: usize = 64 * 1024;
const BYTES_PAYLOAD_OFFSET: u64 = (HEADER_LEN + 4) as u64;

type AnyError = Box<dyn Error>;

fn main() -> Result<(), AnyError> {
    let mut arguments = env::args_os().skip(1);
    let lane = arguments
        .next()
        .and_then(|value| value.into_string().ok())
        .ok_or_else(|| invalid("usage: phase4_fair_benchmark <sqlite|append> <source> <store>"))?;
    let source_path = arguments
        .next()
        .map(PathBuf::from)
        .ok_or_else(|| invalid("missing source path"))?;
    let store_path = arguments
        .next()
        .map(PathBuf::from)
        .ok_or_else(|| invalid("missing store path"))?;
    if arguments.next().is_some() {
        return Err(invalid("unexpected extra argument").into());
    }

    let preflight_start = Instant::now();
    let source = preflight_source(&source_path)?;
    let source_preflight_ns = preflight_start.elapsed().as_nanos();
    ensure_fresh_store(&store_path, &lane)?;
    let prepare_start = Instant::now();
    match lane.as_str() {
        "sqlite" => prepare_sqlite(&store_path)?,
        "append" => prepare_append(&store_path)?,
        _ => return Err(invalid("lane must be `sqlite` or `append`").into()),
    }
    let store_prepare_ns = prepare_start.elapsed().as_nanos();

    let report = match lane.as_str() {
        "sqlite" => run_sqlite(&source_path, &store_path, &source)?,
        "append" => run_append(&source_path, &store_path, &source)?,
        _ => return Err(invalid("lane must be `sqlite` or `append`").into()),
    };
    print_report(
        &source_path,
        &store_path,
        &source,
        source_preflight_ns,
        store_prepare_ns,
        &report,
    );
    Ok(())
}

#[derive(Clone)]
struct ExpectedRange {
    name: &'static str,
    range: Range<u64>,
    bytes: Vec<u8>,
}

struct SourcePreflight {
    bytes: u64,
    raw_fingerprint: [u8; 32],
    logical_v1_fingerprint: [u8; 32],
    ranges: Vec<ExpectedRange>,
}

fn preflight_source(path: &Path) -> Result<SourcePreflight, AnyError> {
    let mut file = File::open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() || metadata.len() != HISTORICAL_SOURCE_BYTES {
        return Err(invalid(format!(
            "historical source must be a regular {HISTORICAL_SOURCE_BYTES}-byte file"
        ))
        .into());
    }
    let mut raw = blake3::Hasher::new();
    let mut logical_v1 = blake3::Hasher::new();
    logical_v1.update(b"logical-v1\0");
    let mut buffer = [0_u8; PREFLIGHT_READER_BYTES];
    let mut counted = 0_u64;
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        raw.update(&buffer[..read]);
        logical_v1.update(&buffer[..read]);
        counted = counted
            .checked_add(u64::try_from(read)?)
            .ok_or_else(|| invalid("source byte count overflow"))?;
    }
    if counted != HISTORICAL_SOURCE_BYTES {
        return Err(invalid("source changed during preflight").into());
    }
    let raw_fingerprint = *raw.finalize().as_bytes();
    let logical_v1_fingerprint = *logical_v1.finalize().as_bytes();
    if hex_digest(&raw_fingerprint) != HISTORICAL_RAW_BLAKE3
        || hex_digest(&logical_v1_fingerprint) != HISTORICAL_LOGICAL_V1_BLAKE3
    {
        return Err(
            invalid("source fingerprint is not the frozen historical random-100m fixture").into(),
        );
    }

    let middle = HISTORICAL_SOURCE_BYTES / 2;
    let specs = [
        ("start", 0..4096),
        ("middle", middle - 2048..middle + 2048),
        (
            "end",
            HISTORICAL_SOURCE_BYTES - 4096..HISTORICAL_SOURCE_BYTES,
        ),
        ("empty", middle..middle),
    ];
    let mut ranges = Vec::with_capacity(specs.len());
    for (name, range) in specs {
        let length = usize::try_from(range.end - range.start)?;
        let mut bytes = vec![0_u8; length];
        file.seek(SeekFrom::Start(range.start))?;
        file.read_exact(&mut bytes)?;
        ranges.push(ExpectedRange { name, range, bytes });
    }
    Ok(SourcePreflight {
        bytes: counted,
        raw_fingerprint,
        logical_v1_fingerprint,
        ranges,
    })
}

fn ensure_fresh_store(path: &Path, lane: &str) -> Result<(), AnyError> {
    ensure_missing(path)?;
    if lane == "sqlite" {
        for suffix in ["-journal", "-wal", "-shm"] {
            let mut sidecar = path.as_os_str().to_os_string();
            sidecar.push(suffix);
            ensure_missing(Path::new(&sidecar))?;
        }
    }
    Ok(())
}

fn ensure_missing(path: &Path) -> Result<(), AnyError> {
    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
        Ok(_) => Err(invalid(format!(
            "refusing to overwrite existing store path {}",
            path.display()
        ))
        .into()),
    }
}

fn prepare_sqlite(path: &Path) -> Result<(), AnyError> {
    let engine = Engine::open(path)?;
    if engine.load_visible_root()?.is_some() {
        return Err(invalid("fresh SQLite store unexpectedly has a visible root").into());
    }
    drop(engine);
    Ok(())
}

fn prepare_append(path: &Path) -> Result<(), AnyError> {
    let engine = AppendOnlyEngine::open(path)?;
    if engine.load_visible_root()?.is_some() {
        return Err(invalid("fresh append store unexpectedly has a visible root").into());
    }
    drop(engine);
    Ok(())
}

#[derive(Default)]
struct CaptureStats {
    callbacks: u64,
    submitted_objects: u64,
    created: u64,
    reused: u64,
    canonical_bytes_submitted: u64,
    raw_chunk_hash_bytes: u64,
    canonical_identity_hash_bytes: u64,
    encode_ns: u128,
    raw_chunk_hash_ns: u128,
    canonical_identity_hash_ns: u128,
    put_ns: u128,
}

fn scan_source<P>(
    source_path: &Path,
    mut put: P,
) -> Result<
    (
        Vec<DirectoryEntry>,
        ExpectedRange,
        CdcCounters,
        CaptureStats,
    ),
    AnyError,
>
where
    P: FnMut(ObjectId, &[u8]) -> EngineResult<PutOutcome>,
{
    let mut entries = Vec::with_capacity(8_192.min(MAX_CHILD_REFERENCES));
    let mut stats = CaptureStats::default();
    let mut engine_failure: Option<EngineError> = None;
    let mut first_boundary: Option<(u64, Vec<u8>)> = None;
    let mut cross_boundary = None;
    let result = FastCdc::new().scan(
        BufReader::with_capacity(SOURCE_READER_BYTES, File::open(source_path)?),
        |chunk| {
            if entries.len() == MAX_CHILD_REFERENCES {
                return Err(CoreError::ObjectLimitExceeded);
            }
            let ordinal = entries.len();
            if ordinal == 0 {
                let boundary = u64::try_from(chunk.len()).map_err(|_| CoreError::LengthOverflow)?;
                let start = chunk
                    .len()
                    .checked_sub(CROSS_BOUNDARY_SIDE_BYTES)
                    .ok_or(CoreError::LengthOverflow)?;
                let prefix = chunk.get(start..).ok_or(CoreError::LengthOverflow)?;
                first_boundary = Some((boundary, prefix.to_vec()));
            } else if ordinal == 1 {
                let (boundary, mut bytes) =
                    first_boundary.take().ok_or(CoreError::LengthOverflow)?;
                let suffix = chunk
                    .get(..CROSS_BOUNDARY_SIDE_BYTES)
                    .ok_or(CoreError::LengthOverflow)?;
                bytes.extend_from_slice(suffix);
                let side = u64::try_from(CROSS_BOUNDARY_SIDE_BYTES)
                    .map_err(|_| CoreError::LengthOverflow)?;
                cross_boundary = Some(ExpectedRange {
                    name: "cross_cdc_boundary",
                    range: boundary
                        .checked_sub(side)
                        .ok_or(CoreError::LengthOverflow)?
                        ..boundary
                            .checked_add(side)
                            .ok_or(CoreError::LengthOverflow)?,
                    bytes,
                });
            }
            let raw_hash_start = Instant::now();
            let raw_id = chunk_id(chunk);
            checked_add_u128(
                &mut stats.raw_chunk_hash_ns,
                raw_hash_start.elapsed().as_nanos(),
            )?;
            checked_add_u64(
                &mut stats.raw_chunk_hash_bytes,
                u64::try_from(chunk.len()).map_err(|_| CoreError::LengthOverflow)?,
            )?;
            let name = CanonicalName::new(&format!("{ordinal:08x}-{:08x}-{raw_id}", chunk.len()))?;

            let encode_start = Instant::now();
            let canonical = encode_object(&Object::bytes(chunk.to_vec())?)?;
            checked_add_u128(&mut stats.encode_ns, encode_start.elapsed().as_nanos())?;
            let identity_start = Instant::now();
            let object_id = ObjectId::for_bytes(&canonical);
            checked_add_u128(
                &mut stats.canonical_identity_hash_ns,
                identity_start.elapsed().as_nanos(),
            )?;
            checked_add_u64(
                &mut stats.canonical_identity_hash_bytes,
                u64::try_from(canonical.len()).map_err(|_| CoreError::LengthOverflow)?,
            )?;

            let put_start = Instant::now();
            let outcome = match put(object_id, &canonical) {
                Ok(outcome) => outcome,
                Err(error) => {
                    engine_failure = Some(error);
                    return Err(CoreError::Io);
                }
            };
            checked_add_u128(&mut stats.put_ns, put_start.elapsed().as_nanos())?;
            record_submission(&mut stats, canonical.len(), outcome)?;
            checked_add_u64(&mut stats.callbacks, 1)?;
            entries.push(DirectoryEntry::new(
                name,
                ObjectReference::new(ObjectKind::Bytes, object_id),
            ));
            Ok(())
        },
    );
    let counters = match result {
        Ok(counters) => counters,
        Err(cdc_error) => {
            return match engine_failure.take() {
                Some(engine_error) => Err(engine_error.into()),
                None => Err(cdc_error.into()),
            };
        }
    };
    if counters.bytes_scanned != HISTORICAL_SOURCE_BYTES
        || counters.chunks_emitted != EXPECTED_CURRENT_CDC_CHUNKS
        || counters.chunks_emitted != stats.callbacks
    {
        return Err(invalid(format!(
            "current Phase 2 CDC mismatch: bytes={}, chunks={}, callbacks={}",
            counters.bytes_scanned, counters.chunks_emitted, stats.callbacks
        ))
        .into());
    }
    let cross_boundary = cross_boundary
        .ok_or_else(|| invalid("Phase 2 CDC did not produce the first two chunks"))?;
    Ok((entries, cross_boundary, counters, stats))
}

fn record_submission(
    stats: &mut CaptureStats,
    canonical_len: usize,
    outcome: PutOutcome,
) -> Result<(), CoreError> {
    checked_add_u64(&mut stats.submitted_objects, 1)?;
    checked_add_u64(
        &mut stats.canonical_bytes_submitted,
        u64::try_from(canonical_len).map_err(|_| CoreError::LengthOverflow)?,
    )?;
    match outcome {
        PutOutcome::Created => checked_add_u64(&mut stats.created, 1),
        PutOutcome::Reused => checked_add_u64(&mut stats.reused, 1),
    }
}

fn checked_add_u64(value: &mut u64, amount: u64) -> Result<(), CoreError> {
    *value = value.checked_add(amount).ok_or(CoreError::LengthOverflow)?;
    Ok(())
}

fn checked_add_u128(value: &mut u128, amount: u128) -> Result<(), CoreError> {
    *value = value.checked_add(amount).ok_or(CoreError::LengthOverflow)?;
    Ok(())
}

fn verify_capture_counts(stats: &CaptureStats) -> Result<(), AnyError> {
    if stats.submitted_objects != EXPECTED_SUBMITTED_OBJECTS
        || stats.created != EXPECTED_CREATED_OBJECTS
        || stats.reused != EXPECTED_REUSED_OBJECTS
    {
        return Err(invalid(format!(
            "frozen capture outcomes differ: submitted={}, created={}, reused={}",
            stats.submitted_objects, stats.created, stats.reused
        ))
        .into());
    }
    Ok(())
}

struct Graph {
    manifest_id: ObjectId,
    manifest_bytes: u64,
    root_directory_id: ObjectId,
    root_directory_bytes: u64,
    root: RootRecord,
    delta: DeltaRecord,
    manifest_entries: u64,
    cross_boundary: ExpectedRange,
}

fn publish_graph<P>(
    entries: Vec<DirectoryEntry>,
    cross_boundary: ExpectedRange,
    source: &SourcePreflight,
    stats: &mut CaptureStats,
    mut put: P,
) -> Result<Graph, AnyError>
where
    P: FnMut(ObjectId, &[u8]) -> EngineResult<PutOutcome>,
{
    let manifest_entries = u64::try_from(entries.len())?;
    let (manifest_id, manifest_bytes) =
        submit_object(Object::directory(entries)?, stats, &mut put)?;
    let root_directory = Object::directory(vec![DirectoryEntry::new(
        CanonicalName::new("source.bin")?,
        ObjectReference::new(ObjectKind::Directory, manifest_id),
    )])?;
    let (root_directory_id, root_directory_bytes) = submit_object(root_directory, stats, &mut put)?;

    let mut root_identity = Vec::with_capacity(128);
    root_identity.extend_from_slice(b"layerfs/phase4-fair-proxy-root-v1\0");
    root_identity.extend_from_slice(&source.bytes.to_be_bytes());
    root_identity.extend_from_slice(&source.raw_fingerprint);
    root_identity.extend_from_slice(&source.logical_v1_fingerprint);
    root_identity.extend_from_slice(root_directory_id.as_bytes());
    let root = RootRecord {
        id: ObjectId::for_bytes(&root_identity),
        directory_object: root_directory_id,
        parent: None,
    };
    let mut delta_payload = Vec::with_capacity(160);
    delta_payload.extend_from_slice(b"layerfs/phase4-fair-proxy-delta-v1\0");
    delta_payload.extend_from_slice(&source.bytes.to_be_bytes());
    delta_payload.extend_from_slice(&source.raw_fingerprint);
    delta_payload.extend_from_slice(&source.logical_v1_fingerprint);
    delta_payload.extend_from_slice(manifest_id.as_bytes());
    delta_payload.extend_from_slice(root_directory_id.as_bytes());
    let delta = DeltaRecord::new(None, root.id, delta_payload);
    Ok(Graph {
        manifest_id,
        manifest_bytes,
        root_directory_id,
        root_directory_bytes,
        root,
        delta,
        manifest_entries,
        cross_boundary,
    })
}

fn submit_object<P>(
    object: Object,
    stats: &mut CaptureStats,
    put: &mut P,
) -> Result<(ObjectId, u64), AnyError>
where
    P: FnMut(ObjectId, &[u8]) -> EngineResult<PutOutcome>,
{
    let encode_start = Instant::now();
    let canonical = encode_object(&object)?;
    checked_add_u128(&mut stats.encode_ns, encode_start.elapsed().as_nanos())?;
    let identity_start = Instant::now();
    let id = ObjectId::for_bytes(&canonical);
    checked_add_u128(
        &mut stats.canonical_identity_hash_ns,
        identity_start.elapsed().as_nanos(),
    )?;
    checked_add_u64(
        &mut stats.canonical_identity_hash_bytes,
        u64::try_from(canonical.len())?,
    )?;
    let put_start = Instant::now();
    let outcome = put(id, &canonical)?;
    checked_add_u128(&mut stats.put_ns, put_start.elapsed().as_nanos())?;
    record_submission(stats, canonical.len(), outcome)?;
    Ok((id, u64::try_from(canonical.len())?))
}

#[derive(Default)]
struct Timings {
    wall_ns: u128,
    open_ns: u128,
    begin_ns: u128,
    cdc_publish_ns: u128,
    graph_publish_ns: u128,
    delta_write_ns: u128,
    commit_ns: u128,
    capture_counter_read_gate_and_handle_drop_ns: u128,
    reopen_and_closure_ns: u128,
    record_loads_and_closures_ns: u128,
    reconstruct_ns: u128,
    logical_ranges_ns: u128,
}

#[derive(Default)]
struct VerifyStats {
    reconstructed_bytes: u64,
    reconstructed_chunks: u64,
    full_range_calls: u64,
    logical_range_calls: u64,
    logical_range_bytes: u64,
    cross_boundary_range_start: u64,
    cross_boundary_range_end: u64,
    cross_boundary_range_calls: u64,
}

enum CounterReport {
    Sqlite {
        capture: EngineCounters,
        reopened: EngineCounters,
        profile: SqliteProfile,
        closure_passes: u64,
    },
    Append {
        capture: AppendOnlyCounters,
        reopened: AppendOnlyCounters,
    },
}

struct RunReport {
    lane: &'static str,
    timings: Timings,
    capture: CaptureStats,
    cdc: CdcCounters,
    graph: Graph,
    verify: VerifyStats,
    counters: CounterReport,
    store_file_bytes: Option<u64>,
    store_allocated_bytes: Option<u64>,
    store_logical_bytes: Option<u64>,
    durability_events: u64,
}

fn run_sqlite(
    source_path: &Path,
    store_path: &Path,
    source: &SourcePreflight,
) -> Result<RunReport, AnyError> {
    let mut timings = Timings::default();
    let wall_start = Instant::now();
    let phase = Instant::now();
    let engine = Engine::open(store_path)?;
    timings.open_ns = phase.elapsed().as_nanos();
    let profile = engine.profile().clone();
    let phase = Instant::now();
    let mut capture = engine.begin_capture(None)?;
    timings.begin_ns = phase.elapsed().as_nanos();
    let phase = Instant::now();
    let (entries, cross_boundary, cdc, mut capture_stats) =
        scan_source(source_path, |id, bytes| {
            capture.put_object_if_absent(id, bytes)
        })?;
    timings.cdc_publish_ns = phase.elapsed().as_nanos();
    let phase = Instant::now();
    let graph = publish_graph(
        entries,
        cross_boundary,
        source,
        &mut capture_stats,
        |id, bytes| capture.put_object_if_absent(id, bytes),
    )?;
    timings.graph_publish_ns = phase.elapsed().as_nanos();
    let phase = Instant::now();
    capture.write_delta(&graph.delta)?;
    timings.delta_write_ns = phase.elapsed().as_nanos();
    let phase = Instant::now();
    capture.commit_root(graph.root.clone())?;
    let mut closure_passes = 0_u64;
    authenticate_proxy_descendants(&engine, &graph)?;
    checked_add_u64(&mut closure_passes, 1)?;
    sync_sqlite_store(store_path)?;
    timings.commit_ns = phase.elapsed().as_nanos();
    let phase = Instant::now();
    let capture_counters = engine.counters()?;
    verify_capture_counts(&capture_stats)?;
    if capture_counters.transactions_started != 1
        || capture_counters.transactions_committed != 1
        || capture_counters.transactions_rolled_back != 0
        || capture_counters.objects_created != EXPECTED_CREATED_OBJECTS
        || capture_counters.objects_reused != EXPECTED_REUSED_OBJECTS
        || !profile.journal_mode.eq_ignore_ascii_case("DELETE")
        || profile.synchronous != 2
        || profile.temp_store != 1
        || profile.mmap_size != 0
    {
        return Err(invalid(
            "SQLite control did not preserve the fixed Phase 4A profile, exact object outcomes, and one FULL transaction",
        )
        .into());
    }
    let durability_events = capture_counters.transactions_committed;
    drop(engine);
    timings.capture_counter_read_gate_and_handle_drop_ns = phase.elapsed().as_nanos();

    let phase = Instant::now();
    let reopened = Engine::open(store_path)?;
    let reopen_visible = reopened.load_visible_root()?;
    if reopen_visible != Some(graph.root.id) {
        return Err(invalid("reopened visible root mismatch").into());
    }
    authenticate_proxy_descendants(&reopened, &graph)?;
    checked_add_u64(&mut closure_passes, 1)?;
    timings.reopen_and_closure_ns = phase.elapsed().as_nanos();
    let phase = Instant::now();
    let visible = reopened.load_visible_root()?;
    authenticate_proxy_descendants(&reopened, &graph)?;
    checked_add_u64(&mut closure_passes, 1)?;
    let root = reopened.load_root(graph.root.id)?;
    authenticate_proxy_descendants(&reopened, &graph)?;
    checked_add_u64(&mut closure_passes, 1)?;
    verify_records(visible, root, reopened.load_delta(graph.delta.id)?, &graph)?;
    if closure_passes != SQLITE_CLOSURE_PASSES {
        return Err(invalid("SQLite closure-pass count mismatch").into());
    }
    timings.record_loads_and_closures_ns = phase.elapsed().as_nanos();
    let mut verify_timings = Timings::default();
    let verify = verify_graph(
        &graph,
        source,
        |id, range| reopened.read_object_range(id, range),
        &mut verify_timings,
    )?;
    timings.reconstruct_ns = verify_timings.reconstruct_ns;
    timings.logical_ranges_ns = verify_timings.logical_ranges_ns;
    timings.wall_ns = wall_start.elapsed().as_nanos();
    let reopened_counters = reopened.counters()?;
    let observations = reopened.observations();
    drop(reopened);

    Ok(RunReport {
        lane: "sqlite_phase4a_plus_postcommit_fullfsync_closure_control",
        timings,
        capture: capture_stats,
        cdc,
        graph,
        verify,
        counters: CounterReport::Sqlite {
            capture: capture_counters,
            reopened: reopened_counters,
            profile,
            closure_passes,
        },
        store_file_bytes: file_bytes(store_path),
        store_allocated_bytes: sqlite_allocated_bytes(store_path),
        store_logical_bytes: observations.logical_engine_bytes,
        durability_events,
    })
}

fn run_append(
    source_path: &Path,
    store_path: &Path,
    source: &SourcePreflight,
) -> Result<RunReport, AnyError> {
    let mut timings = Timings::default();
    let wall_start = Instant::now();
    let phase = Instant::now();
    let engine = AppendOnlyEngine::open(store_path)?;
    timings.open_ns = phase.elapsed().as_nanos();
    let phase = Instant::now();
    let mut capture = engine.begin_capture(None)?;
    timings.begin_ns = phase.elapsed().as_nanos();
    let phase = Instant::now();
    let (entries, cross_boundary, cdc, mut capture_stats) =
        scan_source(source_path, |id, bytes| {
            capture.put_object_if_absent(id, bytes)
        })?;
    timings.cdc_publish_ns = phase.elapsed().as_nanos();
    let phase = Instant::now();
    let graph = publish_graph(
        entries,
        cross_boundary,
        source,
        &mut capture_stats,
        |id, bytes| capture.put_object_if_absent(id, bytes),
    )?;
    timings.graph_publish_ns = phase.elapsed().as_nanos();
    let phase = Instant::now();
    capture.write_delta(&graph.delta)?;
    timings.delta_write_ns = phase.elapsed().as_nanos();
    let phase = Instant::now();
    capture.commit_root(graph.root.clone())?;
    timings.commit_ns = phase.elapsed().as_nanos();
    let phase = Instant::now();
    let capture_counters = engine.counters()?;
    verify_capture_counts(&capture_stats)?;
    if capture_counters.captures_started != 1
        || capture_counters.captures_committed != 1
        || capture_counters.captures_abandoned != 0
        || capture_counters.objects_created != EXPECTED_CREATED_OBJECTS
        || capture_counters.objects_reused != EXPECTED_REUSED_OBJECTS
        || capture_counters.marker_attempts != 1
        || capture_counters.marker_sync_attempts != 1
        || capture_counters.marker_sync_successes != 1
        || capture_counters.marker_sync_failures != 0
    {
        return Err(invalid(
            "append lane did not perform the exact object outcomes and one marker sync",
        )
        .into());
    }
    let store_logical_bytes = capture_counters
        .logical_object_bytes
        .checked_add(capture_counters.logical_root_bytes)
        .and_then(|value| value.checked_add(capture_counters.logical_delta_bytes))
        .ok_or_else(|| invalid("append logical byte observation overflow"))?;
    let durability_events = capture_counters.marker_sync_successes;
    drop(engine);
    timings.capture_counter_read_gate_and_handle_drop_ns = phase.elapsed().as_nanos();

    let phase = Instant::now();
    let reopened = AppendOnlyEngine::open(store_path)?;
    timings.reopen_and_closure_ns = phase.elapsed().as_nanos();
    let phase = Instant::now();
    verify_records(
        reopened.load_visible_root()?,
        reopened.load_root(graph.root.id)?,
        reopened.load_delta(graph.delta.id)?,
        &graph,
    )?;
    timings.record_loads_and_closures_ns = phase.elapsed().as_nanos();
    let mut verify_timings = Timings::default();
    let verify = verify_graph(
        &graph,
        source,
        |id, range| reopened.read_object_range(id, range),
        &mut verify_timings,
    )?;
    timings.reconstruct_ns = verify_timings.reconstruct_ns;
    timings.logical_ranges_ns = verify_timings.logical_ranges_ns;
    timings.wall_ns = wall_start.elapsed().as_nanos();
    let reopened_counters = reopened.counters()?;
    drop(reopened);

    Ok(RunReport {
        lane: "append",
        timings,
        capture: capture_stats,
        cdc,
        graph,
        verify,
        counters: CounterReport::Append {
            capture: capture_counters,
            reopened: reopened_counters,
        },
        store_file_bytes: file_bytes(store_path),
        store_allocated_bytes: allocated_bytes(store_path),
        store_logical_bytes: Some(store_logical_bytes),
        durability_events,
    })
}

fn verify_records(
    visible: Option<ObjectId>,
    root: RootRecord,
    delta: DeltaRecord,
    graph: &Graph,
) -> Result<(), AnyError> {
    if visible != Some(graph.root.id) || root != graph.root || delta != graph.delta {
        return Err(invalid("reopened visible root, root record, or delta record mismatch").into());
    }
    Ok(())
}

fn authenticate_proxy_descendants(engine: &Engine, graph: &Graph) -> Result<(), AnyError> {
    let manifest = engine.read_object_range(graph.manifest_id, 0..graph.manifest_bytes)?;
    let entries = match decode_object(&manifest)? {
        Object::Directory(entries) => entries,
        Object::Bytes(_) => return Err(invalid("source.bin manifest is not a directory").into()),
    };
    if entries.len() > MAX_CHILD_REFERENCES
        || u64::try_from(entries.len())? != graph.manifest_entries
    {
        return Err(invalid("authenticated manifest count is invalid").into());
    }

    let mut descendants = 1_u64;
    for (ordinal, entry) in entries.iter().enumerate() {
        let parsed = parse_manifest_entry(entry, ordinal)?;
        let canonical_len = parsed
            .payload_len
            .checked_add(BYTES_PAYLOAD_OFFSET)
            .ok_or_else(|| invalid("canonical chunk length overflow"))?;
        let canonical = engine.read_object_range(entry.reference().id(), 0..canonical_len)?;
        let bytes = match decode_object(&canonical)? {
            Object::Bytes(bytes) => bytes,
            Object::Directory(_) => return Err(invalid("manifest child is not Bytes").into()),
        };
        if u64::try_from(bytes.len())? != parsed.payload_len
            || chunk_id(&bytes) != parsed.raw_chunk_id
        {
            return Err(invalid("manifest chunk length or raw chunk identity mismatch").into());
        }
        descendants = descendants
            .checked_add(1)
            .ok_or_else(|| invalid("authenticated descendant count overflow"))?;
    }
    if descendants != SQLITE_DESCENDANTS_PER_PASS {
        return Err(invalid("authenticated descendant count mismatch").into());
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn sync_sqlite_store(path: &Path) -> Result<(), AnyError> {
    File::options()
        .read(true)
        .write(true)
        .open(path)?
        .sync_all()?;
    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn sync_sqlite_store(_path: &Path) -> Result<(), AnyError> {
    Err(invalid("SQLite control requires macOS std::fs::File::sync_all F_FULLFSYNC").into())
}

fn verify_graph<R>(
    graph: &Graph,
    source: &SourcePreflight,
    mut read_range: R,
    timings: &mut Timings,
) -> Result<VerifyStats, AnyError>
where
    R: FnMut(ObjectId, Range<u64>) -> EngineResult<Vec<u8>>,
{
    let mut stats = VerifyStats::default();
    let phase = Instant::now();
    let root_bytes = read_full_exact(
        graph.root_directory_id,
        graph.root_directory_bytes,
        &mut read_range,
        &mut stats,
    )?;
    let root_entries = match decode_object(&root_bytes)? {
        Object::Directory(entries) => entries,
        Object::Bytes(_) => return Err(invalid("root object is not a directory").into()),
    };
    let root_entry = root_entries
        .first()
        .filter(|_| root_entries.len() == 1)
        .ok_or_else(|| invalid("root directory does not contain exactly one entry"))?;
    if root_entry.name().as_str() != "source.bin"
        || root_entry.reference() != ObjectReference::new(ObjectKind::Directory, graph.manifest_id)
    {
        return Err(
            invalid("root directory does not contain exact source.bin manifest entry").into(),
        );
    }
    let manifest_bytes = read_full_exact(
        graph.manifest_id,
        graph.manifest_bytes,
        &mut read_range,
        &mut stats,
    )?;
    let entries = match decode_object(&manifest_bytes)? {
        Object::Directory(entries) => entries,
        Object::Bytes(_) => return Err(invalid("source.bin manifest is not a directory").into()),
    };
    if entries.len() > MAX_CHILD_REFERENCES
        || u64::try_from(entries.len())? != graph.manifest_entries
    {
        return Err(invalid("reopened manifest count is invalid").into());
    }

    let mut raw = blake3::Hasher::new();
    let mut logical_v1 = blake3::Hasher::new();
    logical_v1.update(b"logical-v1\0");
    let mut reconstructed = 0_u64;
    for (ordinal, entry) in entries.iter().enumerate() {
        let parsed = parse_manifest_entry(entry, ordinal)?;
        let canonical_len = parsed
            .payload_len
            .checked_add(BYTES_PAYLOAD_OFFSET)
            .ok_or_else(|| invalid("canonical chunk length overflow"))?;
        let canonical = read_range(entry.reference().id(), 0..canonical_len)?;
        stats.full_range_calls = stats
            .full_range_calls
            .checked_add(1)
            .ok_or_else(|| invalid("range-call count overflow"))?;
        let bytes = match decode_object(&canonical)? {
            Object::Bytes(bytes) => bytes,
            Object::Directory(_) => return Err(invalid("manifest child is not Bytes").into()),
        };
        if u64::try_from(bytes.len())? != parsed.payload_len
            || chunk_id(&bytes) != parsed.raw_chunk_id
        {
            return Err(invalid("manifest chunk length or raw chunk identity mismatch").into());
        }
        raw.update(&bytes);
        logical_v1.update(&bytes);
        reconstructed = reconstructed
            .checked_add(parsed.payload_len)
            .ok_or_else(|| invalid("reconstructed byte count overflow"))?;
    }
    if reconstructed != source.bytes
        || raw.finalize().as_bytes() != &source.raw_fingerprint
        || logical_v1.finalize().as_bytes() != &source.logical_v1_fingerprint
    {
        return Err(invalid("streamed reopened source fingerprint mismatch").into());
    }
    stats.reconstructed_bytes = reconstructed;
    stats.reconstructed_chunks = u64::try_from(entries.len())?;
    timings.reconstruct_ns = phase.elapsed().as_nanos();

    let phase = Instant::now();
    let calls_before = stats.logical_range_calls;
    let actual = read_logical_range(
        &entries,
        graph.cross_boundary.range.clone(),
        &mut read_range,
        &mut stats,
    )?;
    let cross_boundary_calls = stats
        .logical_range_calls
        .checked_sub(calls_before)
        .ok_or_else(|| invalid("cross-boundary range-call count underflow"))?;
    if actual.as_slice() != graph.cross_boundary.bytes.as_slice() || cross_boundary_calls < 2 {
        return Err(invalid(
            "logical cross-CDC-boundary range did not read the exact bytes from at least two chunks",
        )
        .into());
    }
    stats.cross_boundary_range_start = graph.cross_boundary.range.start;
    stats.cross_boundary_range_end = graph.cross_boundary.range.end;
    stats.cross_boundary_range_calls = cross_boundary_calls;
    stats.logical_range_bytes = stats
        .logical_range_bytes
        .checked_add(u64::try_from(actual.len())?)
        .ok_or_else(|| invalid("logical range byte count overflow"))?;
    for expected in &source.ranges {
        let actual = read_logical_range(
            &entries,
            expected.range.clone(),
            &mut read_range,
            &mut stats,
        )?;
        if actual != expected.bytes {
            return Err(invalid(format!("logical {} range mismatch", expected.name)).into());
        }
        stats.logical_range_bytes = stats
            .logical_range_bytes
            .checked_add(u64::try_from(actual.len())?)
            .ok_or_else(|| invalid("logical range byte count overflow"))?;
    }
    timings.logical_ranges_ns = phase.elapsed().as_nanos();
    Ok(stats)
}

fn read_full_exact<R>(
    id: ObjectId,
    length: u64,
    read_range: &mut R,
    stats: &mut VerifyStats,
) -> Result<Vec<u8>, AnyError>
where
    R: FnMut(ObjectId, Range<u64>) -> EngineResult<Vec<u8>>,
{
    let bytes = read_range(id, 0..length)?;
    stats.full_range_calls = stats
        .full_range_calls
        .checked_add(1)
        .ok_or_else(|| invalid("range-call count overflow"))?;
    Ok(bytes)
}

struct ParsedManifestEntry {
    payload_len: u64,
    raw_chunk_id: ObjectId,
}

fn parse_manifest_entry(
    entry: &DirectoryEntry,
    expected_ordinal: usize,
) -> Result<ParsedManifestEntry, AnyError> {
    if entry.reference().kind() != ObjectKind::Bytes {
        return Err(invalid("manifest reference is not Bytes").into());
    }
    let mut fields = entry.name().as_str().split('-');
    let ordinal = fields
        .next()
        .ok_or_else(|| invalid("manifest ordinal missing"))?;
    let payload_len = fields
        .next()
        .ok_or_else(|| invalid("manifest payload length missing"))?;
    let raw_chunk_id = fields
        .next()
        .ok_or_else(|| invalid("manifest raw chunk id missing"))?;
    if fields.next().is_some()
        || ordinal.len() != 8
        || payload_len.len() != 8
        || usize::from_str_radix(ordinal, 16)? != expected_ordinal
    {
        return Err(invalid("manifest name is not canonical ordinal-length-id").into());
    }
    let payload_len = u64::from_str_radix(payload_len, 16)?;
    if payload_len == 0 || payload_len > u64::try_from(MAXIMUM_CHUNK_BYTES)? {
        return Err(invalid("manifest chunk length is outside Phase 2 bounds").into());
    }
    Ok(ParsedManifestEntry {
        payload_len,
        raw_chunk_id: raw_chunk_id.parse()?,
    })
}

fn read_logical_range<R>(
    entries: &[DirectoryEntry],
    range: Range<u64>,
    read_range: &mut R,
    stats: &mut VerifyStats,
) -> Result<Vec<u8>, AnyError>
where
    R: FnMut(ObjectId, Range<u64>) -> EngineResult<Vec<u8>>,
{
    if range.start > range.end || range.end > HISTORICAL_SOURCE_BYTES {
        return Err(invalid("invalid logical source range").into());
    }
    let mut output = Vec::with_capacity(usize::try_from(range.end - range.start)?);
    let mut logical_start = 0_u64;
    for (ordinal, entry) in entries.iter().enumerate() {
        let parsed = parse_manifest_entry(entry, ordinal)?;
        let logical_end = logical_start
            .checked_add(parsed.payload_len)
            .ok_or_else(|| invalid("logical chunk offset overflow"))?;
        if range.start == range.end {
            if range.start >= logical_start && range.start <= logical_end {
                let local = range.start - logical_start;
                let canonical = BYTES_PAYLOAD_OFFSET
                    .checked_add(local)
                    .ok_or_else(|| invalid("canonical empty range overflow"))?;
                let bytes = read_range(entry.reference().id(), canonical..canonical)?;
                if !bytes.is_empty() {
                    return Err(invalid("empty object range returned bytes").into());
                }
                stats.logical_range_calls = stats
                    .logical_range_calls
                    .checked_add(1)
                    .ok_or_else(|| invalid("logical range-call count overflow"))?;
                return Ok(output);
            }
        } else if range.start < logical_end && range.end > logical_start {
            let overlap_start = range.start.max(logical_start) - logical_start;
            let overlap_end = range.end.min(logical_end) - logical_start;
            let canonical_start = BYTES_PAYLOAD_OFFSET
                .checked_add(overlap_start)
                .ok_or_else(|| invalid("canonical range start overflow"))?;
            let canonical_end = BYTES_PAYLOAD_OFFSET
                .checked_add(overlap_end)
                .ok_or_else(|| invalid("canonical range end overflow"))?;
            output.extend_from_slice(&read_range(
                entry.reference().id(),
                canonical_start..canonical_end,
            )?);
            stats.logical_range_calls = stats
                .logical_range_calls
                .checked_add(1)
                .ok_or_else(|| invalid("logical range-call count overflow"))?;
        }
        logical_start = logical_end;
        if logical_start >= range.end && range.start != range.end {
            break;
        }
    }
    if u64::try_from(output.len())? != range.end - range.start {
        return Err(invalid("logical range reconstruction was short").into());
    }
    Ok(output)
}

fn print_report(
    source_path: &Path,
    store_path: &Path,
    source: &SourcePreflight,
    source_preflight_ns: u128,
    store_prepare_ns: u128,
    report: &RunReport,
) {
    println!("benchmark=phase4_backend_cas_proxy_diagnostic");
    println!("benchmark_status=exploratory_backend_cas_proxy_not_a_decision_row");
    println!("promotion_authorized=false");
    println!("full_logical_workload=false");
    println!("target_attainment_authorized=false");
    println!("phase3_semantic_persistence=false");
    println!("phase3_limitation=deterministic_proxy_delta_and_directory_of_chunks_only");
    println!("lane={}", report.lane);
    println!("source_path={}", source_path.display());
    println!("store_path={}", store_path.display());
    println!("source_generation=false");
    println!("source_bytes={}", source.bytes);
    println!("source_raw_blake3={}", hex_digest(&source.raw_fingerprint));
    println!(
        "source_historical_logical_v1_blake3={}",
        hex_digest(&source.logical_v1_fingerprint)
    );
    println!("source_expected_sha256={HISTORICAL_SHA256}");
    println!("source_sha256_recomputed=false");
    println!("source_preflight_ns={source_preflight_ns}");
    println!("source_preflight_in_backend_cas_proxy_timer=false");
    println!("store_prepare_ns={store_prepare_ns}");
    println!("store_prepare_in_backend_cas_proxy_timer=false");
    println!("backend_cas_proxy_timer_cpu_ns=unavailable");
    println!("backend_cas_proxy_timer_rss_bytes=unavailable");
    println!("external_time_boundary=process_start_including_fixture_preflight_and_store_prepare");
    println!("backend_cas_proxy_timer_boundary=prepared_engine_open_through_composite_commit_and_closure_counter_read_drop_reopen_and_closure_authenticated_record_loads_and_closures_full_stream_reconstruction_and_exact_logical_ranges");
    println!("backend_cas_proxy_wall_ns={}", report.timings.wall_ns);
    println!(
        "backend_cas_proxy_mib_per_s={:.6}",
        100.0 * 1_000_000_000.0 / report.timings.wall_ns as f64
    );
    println!("phase.open_ns={}", report.timings.open_ns);
    println!("phase.begin_ns={}", report.timings.begin_ns);
    println!("phase.cdc_publish_ns={}", report.timings.cdc_publish_ns);
    println!("phase.graph_publish_ns={}", report.timings.graph_publish_ns);
    println!("phase.delta_write_ns={}", report.timings.delta_write_ns);
    println!("phase.commit_ns={}", report.timings.commit_ns);
    println!(
        "phase.commit_boundary={}",
        if report.lane == "sqlite_phase4a_plus_postcommit_fullfsync_closure_control" {
            "phase4a_full_transaction_commit_then_postpublication_descendant_authentication_then_explicit_full_filesystem_sync"
        } else {
            "append_marker_publication_recursive_closure_and_full_filesystem_sync"
        }
    );
    println!(
        "phase.capture_counter_read_gate_and_handle_drop_ns={}",
        report.timings.capture_counter_read_gate_and_handle_drop_ns
    );
    println!(
        "phase.reopen_and_closure_ns={}",
        report.timings.reopen_and_closure_ns
    );
    println!(
        "phase.record_loads_and_closures_ns={}",
        report.timings.record_loads_and_closures_ns
    );
    println!("phase.reconstruct_ns={}", report.timings.reconstruct_ns);
    println!(
        "phase.logical_ranges_ns={}",
        report.timings.logical_ranges_ns
    );
    println!("durability_events={}", report.durability_events);
    println!("durability_expected_events=1");
    println!("durability_exact_event_gate_passed=true");
    println!("durability_event_unit=one_capture_publication_composite_not_sync_syscall_count");
    println!(
        "durability_event_kind={}",
        if report.lane == "sqlite_phase4a_plus_postcommit_fullfsync_closure_control" {
            "sqlite_phase4a_full_transaction_then_postpublication_closure_then_explicit_full_filesystem_sync"
        } else {
            "append_marker_full_filesystem_sync"
        }
    );
    println!("cdc_bytes_scanned={}", report.cdc.bytes_scanned);
    println!("cdc_chunks={}", report.cdc.chunks_emitted);
    println!("manifest_entries={}", report.graph.manifest_entries);
    println!("manifest_entry_bound={MAX_CHILD_REFERENCES}");
    println!("manifest_canonical_bytes={}", report.graph.manifest_bytes);
    println!("manifest_object_byte_bound={MAX_OBJECT_BYTES}");
    println!(
        "root_directory_canonical_bytes={}",
        report.graph.root_directory_bytes
    );
    println!("manifest_id={}", report.graph.manifest_id);
    println!("root_directory_id={}", report.graph.root_directory_id);
    println!("root_id={}", report.graph.root.id);
    println!("delta_id={}", report.graph.delta.id);
    println!("closure_graph=root_directory/source.bin/chunk_manifest/bytes_chunks");
    println!(
        "capture.submitted_objects={}",
        report.capture.submitted_objects
    );
    println!("capture.objects_created={}", report.capture.created);
    println!("capture.objects_reused={}", report.capture.reused);
    println!("capture.exact_outcome_gate_passed=true");
    println!("capture.expected_submitted_objects={EXPECTED_SUBMITTED_OBJECTS}");
    println!("capture.expected_objects_created={EXPECTED_CREATED_OBJECTS}");
    println!("capture.expected_objects_reused={EXPECTED_REUSED_OBJECTS}");
    println!(
        "capture.canonical_bytes_submitted={}",
        report.capture.canonical_bytes_submitted
    );
    println!("capture.encode_ns={}", report.capture.encode_ns);
    println!(
        "capture.raw_chunk_hash_ns={}",
        report.capture.raw_chunk_hash_ns
    );
    println!(
        "capture.raw_chunk_hash_bytes={}",
        report.capture.raw_chunk_hash_bytes
    );
    println!(
        "capture.canonical_identity_hash_ns={}",
        report.capture.canonical_identity_hash_ns
    );
    println!(
        "capture.canonical_identity_hash_bytes={}",
        report.capture.canonical_identity_hash_bytes
    );
    println!("capture.put_ns={}", report.capture.put_ns);
    println!(
        "verify.reconstructed_bytes={}",
        report.verify.reconstructed_bytes
    );
    println!(
        "verify.reconstructed_chunks={}",
        report.verify.reconstructed_chunks
    );
    println!("verify.full_range_calls={}", report.verify.full_range_calls);
    println!(
        "verify.logical_range_calls={}",
        report.verify.logical_range_calls
    );
    println!(
        "verify.logical_range_bytes={}",
        report.verify.logical_range_bytes
    );
    println!("verify.object_length_calls=0");
    println!(
        "verify.cross_boundary_range_start={}",
        report.verify.cross_boundary_range_start
    );
    println!(
        "verify.cross_boundary_range_end={}",
        report.verify.cross_boundary_range_end
    );
    println!(
        "verify.cross_boundary_range_calls={}",
        report.verify.cross_boundary_range_calls
    );
    println!("harness.full_source_staging=false");
    println!(
        "harness.single_chunk_canonical_read_bound_bytes={}",
        u64::try_from(MAXIMUM_CHUNK_BYTES)
            .ok()
            .and_then(|value| value.checked_add(BYTES_PAYLOAD_OFFSET))
            .map_or_else(|| "unavailable".to_owned(), |value| value.to_string())
    );
    println!("harness.graph_object_read_bound_bytes={MAX_OBJECT_BYTES}");
    println!("process_memory_high_water_bytes=unavailable");
    println!("harness.manifest_metadata_bound_entries={MAX_CHILD_REFERENCES}");
    println!("harness.unbounded_object_id_map=false");
    println!("harness.unbounded_object_byte_cache=false");
    println!("source_os_cache_state=uncontrolled_warm_or_unknown_after_required_full_preflight");
    println!("store_os_cache_state=uncontrolled_warm_or_unknown_after_prepare_not_cold_apfs");
    println!("store_file_bytes={}", option_u64(report.store_file_bytes));
    println!(
        "store_allocated_bytes={}",
        option_u64(report.store_allocated_bytes)
    );
    println!(
        "store_logical_bytes={}",
        option_u64(report.store_logical_bytes)
    );
    println!("physical_io_bytes=unavailable");
    println!("iterations=1_external_campaign_must_supply_warmup_and_at_least_three_retained");
    match &report.counters {
        CounterReport::Sqlite {
            capture,
            reopened,
            profile,
            closure_passes,
        } => {
            println!(
                "sqlite.control_name=sqlite_phase4a_plus_postcommit_fullfsync_closure_control"
            );
            println!("sqlite.control_authoritative=false");
            println!("sqlite.phase4a_engine_profile_unchanged=true");
            println!("sqlite.internal_fullfsync=false");
            println!("sqlite.explicit_postcommit_fullfsync=true");
            println!("sqlite.fullfsync_platform_gate=macos_std_file_sync_all_f_fullfsync");
            println!("sqlite.fullfsync_platform_gate_passed=true");
            println!("sqlite.closure_order=postpublication_benchmark_composition");
            println!("sqlite.closure_passes={closure_passes}");
            println!("sqlite.closure_expected_passes={SQLITE_CLOSURE_PASSES}");
            println!("sqlite.closure_descendants_per_pass={SQLITE_DESCENDANTS_PER_PASS}");
            println!("sqlite.closure_exact_gate_passed=true");
            println!("sqlite.closure_object_cache=false");
            println!("sqlite.profile.journal_mode={}", profile.journal_mode);
            println!("sqlite.profile.synchronous={}", profile.synchronous);
            println!("sqlite.profile.temp_store={}", profile.temp_store);
            println!("sqlite.profile.mmap_size={}", profile.mmap_size);
            print_sqlite_counters("capture_engine", capture);
            print_sqlite_counters("reopen_engine", reopened);
        }
        CounterReport::Append { capture, reopened } => {
            print_append_counters("capture_engine", capture);
            print_append_counters("reopen_engine", reopened);
        }
    }
}

fn print_sqlite_counters(label: &str, counters: &EngineCounters) {
    println!(
        "{label}.transactions_started={}",
        counters.transactions_started
    );
    println!(
        "{label}.transactions_committed={}",
        counters.transactions_committed
    );
    println!(
        "{label}.transactions_rolled_back={}",
        counters.transactions_rolled_back
    );
    println!("{label}.statements={}", counters.statements);
    println!("{label}.busy_events={}", counters.busy_events);
    println!("{label}.locked_events={}", counters.locked_events);
    println!("{label}.objects_validated={}", counters.objects_validated);
    println!("{label}.objects_created={}", counters.objects_created);
    println!("{label}.objects_reused={}", counters.objects_reused);
    println!("{label}.object_bytes_read={}", counters.object_bytes_read);
    println!(
        "{label}.object_bytes_written={}",
        counters.object_bytes_written
    );
    println!(
        "{label}.range_bytes_requested={}",
        counters.range_bytes_requested
    );
    println!(
        "{label}.range_bytes_returned={}",
        counters.range_bytes_returned
    );
    println!(
        "{label}.logical_object_bytes={}",
        counters.logical_object_bytes
    );
    println!("{label}.logical_root_bytes={}", counters.logical_root_bytes);
    println!(
        "{label}.logical_delta_bytes={}",
        counters.logical_delta_bytes
    );
}

fn print_append_counters(label: &str, counters: &AppendOnlyCounters) {
    println!("{label}.captures_started={}", counters.captures_started);
    println!("{label}.captures_committed={}", counters.captures_committed);
    println!("{label}.captures_abandoned={}", counters.captures_abandoned);
    println!("{label}.frames_appended={}", counters.frames_appended);
    println!("{label}.frames_scanned={}", counters.frames_scanned);
    println!("{label}.frames_recovered={}", counters.frames_recovered);
    println!(
        "{label}.frame_bytes_appended={}",
        counters.frame_bytes_appended
    );
    println!("{label}.object_validated={}", counters.object_validated);
    println!("{label}.objects_created={}", counters.objects_created);
    println!("{label}.objects_reused={}", counters.objects_reused);
    println!("{label}.object_bytes_read={}", counters.object_bytes_read);
    println!(
        "{label}.object_bytes_written={}",
        counters.object_bytes_written
    );
    println!(
        "{label}.object_frame_bytes_written={}",
        counters.object_frame_bytes_written
    );
    println!(
        "{label}.index_frame_bytes_written={}",
        counters.index_frame_bytes_written
    );
    println!(
        "{label}.root_frame_bytes_written={}",
        counters.root_frame_bytes_written
    );
    println!(
        "{label}.delta_frame_bytes_written={}",
        counters.delta_frame_bytes_written
    );
    println!(
        "{label}.marker_frame_bytes_written={}",
        counters.marker_frame_bytes_written
    );
    println!("{label}.carrier_read_calls={}", counters.carrier_read_calls);
    println!(
        "{label}.carrier_write_calls={}",
        counters.carrier_write_calls
    );
    println!("{label}.carrier_bytes_read={}", counters.carrier_bytes_read);
    println!(
        "{label}.carrier_bytes_written={}",
        counters.carrier_bytes_written
    );
    println!("{label}.carrier_append_ns={}", counters.carrier_append_ns);
    println!(
        "{label}.carrier_flush_calls={}",
        counters.carrier_flush_calls
    );
    println!(
        "{label}.carrier_flush_failures={}",
        counters.carrier_flush_failures
    );
    println!("{label}.carrier_flush_ns={}", counters.carrier_flush_ns);
    println!("{label}.object_hash_ns={}", counters.object_hash_ns);
    println!("{label}.object_hash_bytes={}", counters.object_hash_bytes);
    println!(
        "{label}.object_validation_ns={}",
        counters.object_validation_ns
    );
    println!(
        "{label}.object_validation_bytes={}",
        counters.object_validation_bytes
    );
    println!("{label}.object_compare_ns={}", counters.object_compare_ns);
    println!(
        "{label}.object_compare_bytes={}",
        counters.object_compare_bytes
    );
    println!("{label}.object_auth_ns={}", counters.object_auth_ns);
    println!("{label}.index_lookups={}", counters.index_lookups);
    println!("{label}.index_lookup_ns={}", counters.index_lookup_ns);
    println!("{label}.index_page_reads={}", counters.index_page_reads);
    println!("{label}.index_cache_hits={}", counters.index_cache_hits);
    println!("{label}.index_cache_misses={}", counters.index_cache_misses);
    println!(
        "{label}.index_cache_evictions={}",
        counters.index_cache_evictions
    );
    println!("{label}.marker_attempts={}", counters.marker_attempts);
    println!(
        "{label}.marker_sync_attempts={}",
        counters.marker_sync_attempts
    );
    println!(
        "{label}.marker_sync_successes={}",
        counters.marker_sync_successes
    );
    println!(
        "{label}.marker_sync_failures={}",
        counters.marker_sync_failures
    );
    println!("{label}.marker_sync_ns={}", counters.marker_sync_ns);
    println!("{label}.markers_recovered={}", counters.markers_recovered);
    println!("{label}.reopen_scans={}", counters.reopen_scans);
    println!("{label}.index_root_reads={}", counters.index_root_reads);
    println!("{label}.root_reads={}", counters.root_reads);
    println!("{label}.delta_reads={}", counters.delta_reads);
    println!("{label}.marker_reads={}", counters.marker_reads);
    println!(
        "{label}.marker_capture_digest_ns={}",
        counters.marker_capture_digest_ns
    );
    println!(
        "{label}.marker_capture_digest_bytes={}",
        counters.marker_capture_digest_bytes
    );
    println!(
        "{label}.recovery_torn_bytes={}",
        counters.recovery_torn_bytes
    );
    println!(
        "{label}.recovery_malformed_bytes={}",
        counters.recovery_malformed_bytes
    );
    println!(
        "{label}.recovery_integrity_bytes={}",
        counters.recovery_integrity_bytes
    );
    println!(
        "{label}.writer_lock_wait_ns={}",
        counters.writer_lock_wait_ns
    );
    println!(
        "{label}.writer_lock_hold_ns={}",
        counters.writer_lock_hold_ns
    );
    println!("{label}.closure_objects={}", counters.closure_objects);
    println!("{label}.closure_references={}", counters.closure_references);
    println!(
        "{label}.range_bytes_requested={}",
        counters.range_bytes_requested
    );
    println!(
        "{label}.range_bytes_returned={}",
        counters.range_bytes_returned
    );
    println!("{label}.residue_bytes={}", counters.residue_bytes);
    println!(
        "{label}.logical_object_bytes={}",
        counters.logical_object_bytes
    );
    println!("{label}.logical_root_bytes={}", counters.logical_root_bytes);
    println!(
        "{label}.logical_delta_bytes={}",
        counters.logical_delta_bytes
    );
}

fn file_bytes(path: &Path) -> Option<u64> {
    fs::metadata(path).ok().map(|metadata| metadata.len())
}

#[cfg(unix)]
fn allocated_bytes(path: &Path) -> Option<u64> {
    use std::os::unix::fs::MetadataExt;
    fs::metadata(path).ok()?.blocks().checked_mul(512)
}

#[cfg(not(unix))]
fn allocated_bytes(_path: &Path) -> Option<u64> {
    None
}

fn sqlite_allocated_bytes(path: &Path) -> Option<u64> {
    let mut total = allocated_bytes(path)?;
    for suffix in ["-journal", "-wal", "-shm"] {
        let mut sidecar = path.as_os_str().to_os_string();
        sidecar.push(suffix);
        if let Some(bytes) = allocated_bytes(Path::new(&sidecar)) {
            total = total.checked_add(bytes)?;
        }
    }
    Some(total)
}

fn option_u64(value: Option<u64>) -> String {
    value.map_or_else(|| "unavailable".to_owned(), |value| value.to_string())
}

fn hex_digest(digest: &[u8; 32]) -> String {
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}
