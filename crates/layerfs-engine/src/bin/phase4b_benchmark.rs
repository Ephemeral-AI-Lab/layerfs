use layerfs_core::{cdc::FastCdc, encode_object, CoreError, Object, ObjectId};
use layerfs_engine::{AppendOnlyCounters, AppendOnlyEngine, DeltaRecord, EngineError, RootRecord};
use std::env;
use std::fs::{self, File};
use std::io::{BufReader, Write};
use std::path::{Path, PathBuf};
use std::time::Instant;

const SOURCE_BUFFER_BYTES: usize = 64 * 1024;
const SOURCE_READER_BYTES: usize = 64 * 1024;
const CDC_BUFFER_BYTES: usize = 32 * 1024;
const MAX_CANONICAL_BYTES: usize = CDC_BUFFER_BYTES + 13;
const CARRIER_BUFFER_BYTES: usize = 64 * 1024;
const PEAK_IN_FLIGHT_BYTES: usize =
    SOURCE_READER_BYTES + (CDC_BUFFER_BYTES * 3) + MAX_CANONICAL_BYTES + CARRIER_BUFFER_BYTES;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let benchmark_start = Instant::now();
    let mut arguments = env::args().skip(1);
    let path = arguments.next().map_or_else(
        || {
            env::temp_dir().join(format!(
                "layerfs-phase4b-benchmark-{}.log",
                std::process::id()
            ))
        },
        Into::into,
    );
    let mib = arguments
        .next()
        .map_or(Ok(8_usize), |value| value.parse::<usize>())?;
    let source_bytes = mib.checked_mul(1024 * 1024).ok_or("source size overflow")?;
    let source_path = source_path(&path);
    let _ = fs::remove_file(&path);
    let _ = fs::remove_file(&source_path);

    let source_generation_start = Instant::now();
    let source_fingerprint = generate_source(&source_path, source_bytes)?;
    let source_generation_ns = source_generation_start.elapsed().as_nanos();

    let open_start = Instant::now();
    let engine = AppendOnlyEngine::open(&path)?;
    let initial_open_ns = open_start.elapsed().as_nanos();
    let (directory_id, directory_bytes) = empty_directory();
    let mut capture = engine.begin_capture(None)?;
    let setup_start = Instant::now();
    capture.put_object_if_absent(directory_id, &directory_bytes)?;
    let setup_ns = setup_start.elapsed().as_nanos();
    let before_scan = capture.counters();

    let cdc_start = Instant::now();
    let mut first_object = None;
    let mut chunk_count = 0_u64;
    let mut callback_ns = 0_u128;
    let mut encode_ns = 0_u128;
    let mut put_ns = 0_u128;
    let mut canonical_bytes_streamed = 0_u64;
    let mut engine_failure: Option<EngineError> = None;
    let cdc_result = FastCdc::new().scan(
        BufReader::with_capacity(SOURCE_READER_BYTES, File::open(&source_path)?),
        |chunk| {
            let callback_start = Instant::now();
            let encode_start = Instant::now();
            let canonical = encode_object(&Object::bytes(chunk.to_vec())?)?;
            encode_ns = encode_ns.saturating_add(encode_start.elapsed().as_nanos());
            canonical_bytes_streamed =
                canonical_bytes_streamed.saturating_add(canonical.len() as u64);
            let put_start = Instant::now();
            let (id, _put_result) = match capture.put_canonical_object_if_absent(&canonical) {
                Ok(result) => result,
                Err(error) => {
                    engine_failure = Some(error);
                    return Err(CoreError::Io);
                }
            };
            if first_object.is_none() {
                first_object = Some((id, canonical.clone()));
            }
            put_ns = put_ns.saturating_add(put_start.elapsed().as_nanos());
            chunk_count += 1;
            callback_ns = callback_ns.saturating_add(callback_start.elapsed().as_nanos());
            Ok(())
        },
    );
    let cdc_pass_ns = cdc_start.elapsed().as_nanos();
    let cdc = match cdc_result {
        Ok(counters) => counters,
        Err(error) => {
            if let Some(engine_error) = engine_failure {
                return Err(Box::new(engine_error));
            }
            return Err(Box::new(error));
        }
    };
    let after_scan = capture.counters();
    let engine_object_hash_bytes = after_scan
        .object_hash_bytes
        .saturating_sub(before_scan.object_hash_bytes);
    if engine_object_hash_bytes != canonical_bytes_streamed {
        return Err(format!(
            "engine hashed {engine_object_hash_bytes} canonical bytes, expected {canonical_bytes_streamed}"
        )
        .into());
    }

    let root = RootRecord {
        id: ObjectId::for_bytes(b"phase4b-benchmark-root"),
        directory_object: directory_id,
        parent: None,
    };
    let delta = DeltaRecord::new(None, root.id, b"phase4b-benchmark".to_vec());
    let delta_start = Instant::now();
    capture.write_delta(&delta)?;
    let delta_prepare_ns = delta_start.elapsed().as_nanos();
    let before_commit = capture.counters();
    let commit_start = Instant::now();
    capture.commit_root(root.clone())?;
    let commit_root_ns = commit_start.elapsed().as_nanos();
    let counters = engine.counters()?;
    let observation = engine.observations()?;

    drop(engine);
    let reopen_start = Instant::now();
    let reopened = AppendOnlyEngine::open(&path)?;
    let reopen_open_scan_ns = reopen_start.elapsed().as_nanos();
    let (first_id, first_bytes) = first_object.ok_or("CDC emitted no chunks")?;
    let validate_start = Instant::now();
    assert_eq!(reopened.load_visible_root()?, Some(root.id));
    assert_eq!(
        reopened.read_object_range(first_id, 0..first_bytes.len() as u64)?,
        first_bytes
    );
    let middle_start = (first_bytes.len() / 3) as u64;
    let middle_end = (middle_start as usize + first_bytes.len().min(4096)) as u64;
    assert_eq!(
        reopened.read_object_range(first_id, middle_start..middle_end)?,
        first_bytes[middle_start as usize..middle_end as usize]
    );
    assert_eq!(
        reopened.read_object_range(first_id, 0..0)?,
        Vec::<u8>::new()
    );
    let reopen_validate_read_ns = validate_start.elapsed().as_nanos();
    let reopen_counters = reopened.counters()?;
    let wall_ns = benchmark_start.elapsed().as_nanos();

    println!("phase4b.status=exploratory-candidate");
    println!("phase4b.production_promotion=not-authorized");
    println!("logical_workload=scanner_admission_only");
    println!("logical_workload_definition=CDC_chunks_are_admitted_but_committed_root_references_empty_directory_and_fixed_benchmark_delta");
    println!("phase4a_comparable=false");
    println!("phase4a_comparison_blocker=full_source_referencing_root_and_closure_not_published");
    println!("phase4b.path={}", path.display());
    println!("source_mode=file_stream");
    println!("source_path={}", source_path.display());
    println!("source_bytes={source_bytes}");
    println!(
        "source_fingerprint_blake3={}",
        hex_digest(&source_fingerprint)
    );
    println!("source_generation_ns={source_generation_ns}");
    println!("initial_open_ns={initial_open_ns}");
    println!("setup_directory_ns={setup_ns}");
    println!("cdc_pass_ns={cdc_pass_ns}");
    println!(
        "cdc_dispatch_and_source_read_ns={}",
        cdc_pass_ns.saturating_sub(callback_ns)
    );
    println!("cdc_callback_ns={callback_ns}");
    println!("object_encode_ns={encode_ns}");
    println!("harness_identity_hash_ns=0");
    println!("harness_identity_hash_bytes=0");
    println!("engine_derives_identity=true");
    println!("engine_object_hash_bytes={engine_object_hash_bytes}");
    println!("canonical_bytes_streamed={canonical_bytes_streamed}");
    println!("identity_hash_passes=one_engine_pass");
    println!("engine_put_wall_ns={put_ns}");
    println!("delta_prepare_ns={delta_prepare_ns}");
    println!("commit_root_only_ns={commit_root_ns}");
    println!("commit_root_only_scope=publication_authentication_flush_and_sync");
    println!("reopen_open_scan_ns={reopen_open_scan_ns}");
    println!("reopen_validate_read_ns={reopen_validate_read_ns}");
    println!("wall_ns={wall_ns}");
    println!("cpu_ns=unavailable");
    println!("rss_peak_bytes=unavailable");
    println!("iterations=1");
    println!("median_wall_ns=unavailable_single_run");
    println!("spread_wall_ns=unavailable_single_run");
    println!("cdc_bytes_scanned={}", cdc.bytes_scanned);
    println!("cdc_chunks={}", cdc.chunks_emitted);
    println!("ingest_chunk_callbacks={chunk_count}");
    print_counter_delta("ingest_counter_delta", before_scan, after_scan);
    print_counter_delta("commit_counter_delta", before_commit, counters);
    print_counters("cumulative", &counters);
    print_counters("reopen_cumulative", &reopen_counters);
    println!("carrier_bytes={}", observation.carrier_bytes);
    println!("visible_end={}", observation.visible_end);
    println!("residue_bytes={}", observation.residue_bytes);
    println!("logical_object_bytes={}", observation.logical_object_bytes);
    println!("logical_delta_bytes={}", observation.logical_delta_bytes);
    println!("reopen_cache_capacity_pages=32");
    println!("reopen_engine_cache_state=empty_at_open_then_bounded_32_page");
    println!("reopen_os_cache_state=warm_or_unknown_not_cold_apfs");
    println!("concurrency_mode=single_writer_serialized");
    println!("source_staging_bytes=0");
    println!("source_staging_definition=in_memory_only;source_file_is_input_not_carrier_staging");
    println!("source_file_storage_bytes={source_bytes}");
    println!("source_generator_buffer_bytes={SOURCE_BUFFER_BYTES}");
    println!("source_reader_buffer_bytes={SOURCE_READER_BYTES}");
    println!("cdc_internal_buffer_bytes={CDC_BUFFER_BYTES}");
    println!("cdc_callback_copy_bytes={CDC_BUFFER_BYTES}");
    println!("canonical_object_buffer_bytes={MAX_CANONICAL_BYTES}");
    println!("carrier_write_buffer_bytes={CARRIER_BUFFER_BYTES}");
    println!("peak_in_flight_bytes={PEAK_IN_FLIGHT_BYTES}");
    println!("peak_in_flight_basis=bounded_capacity_upper_bound_not_allocator_sample");
    println!("actual_syscalls=unavailable");
    println!("sqlite_pager_vfs_journal_bytes=not_applicable_phase4b");
    println!("phase4a_decision=required_before_promotion");
    println!("source_fixture=generated_lcg_file;not_the_historical_sqlite_xorshift_fixture");

    drop(reopened);
    fs::remove_file(path)?;
    fs::remove_file(source_path)?;
    Ok(())
}

fn source_path(path: &Path) -> PathBuf {
    path.with_extension("source")
}

fn generate_source(path: &Path, length: usize) -> Result<[u8; 32], Box<dyn std::error::Error>> {
    let mut file = File::create(path)?;
    let mut hasher = blake3::Hasher::new();
    let mut buffer = [0_u8; SOURCE_BUFFER_BYTES];
    let mut random = 0x9e37_79b9_u32;
    let mut remaining = length;
    while remaining != 0 {
        let amount = remaining.min(buffer.len());
        for byte in &mut buffer[..amount] {
            random = random.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            *byte = (random >> 24) as u8;
        }
        file.write_all(&buffer[..amount])?;
        hasher.update(&buffer[..amount]);
        remaining -= amount;
    }
    file.flush()?;
    Ok(*hasher.finalize().as_bytes())
}

fn hex_digest(digest: &[u8; 32]) -> String {
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn print_counter_delta(label: &str, before: AppendOnlyCounters, after: AppendOnlyCounters) {
    println!(
        "{label}.carrier_read_calls={}",
        after
            .carrier_read_calls
            .saturating_sub(before.carrier_read_calls)
    );
    println!(
        "{label}.carrier_write_calls={}",
        after
            .carrier_write_calls
            .saturating_sub(before.carrier_write_calls)
    );
    println!(
        "{label}.carrier_bytes_read={}",
        after
            .carrier_bytes_read
            .saturating_sub(before.carrier_bytes_read)
    );
    println!(
        "{label}.carrier_bytes_written={}",
        after
            .carrier_bytes_written
            .saturating_sub(before.carrier_bytes_written)
    );
    println!(
        "{label}.carrier_append_ns={}",
        after
            .carrier_append_ns
            .saturating_sub(before.carrier_append_ns)
    );
    println!(
        "{label}.carrier_flush_calls={}",
        after
            .carrier_flush_calls
            .saturating_sub(before.carrier_flush_calls)
    );
    println!(
        "{label}.carrier_flush_ns={}",
        after
            .carrier_flush_ns
            .saturating_sub(before.carrier_flush_ns)
    );
    println!(
        "{label}.carrier_flush_failures={}",
        after
            .carrier_flush_failures
            .saturating_sub(before.carrier_flush_failures)
    );
    println!(
        "{label}.object_hash_ns={}",
        after.object_hash_ns.saturating_sub(before.object_hash_ns)
    );
    println!(
        "{label}.object_hash_bytes={}",
        after
            .object_hash_bytes
            .saturating_sub(before.object_hash_bytes)
    );
    println!(
        "{label}.object_validation_ns={}",
        after
            .object_validation_ns
            .saturating_sub(before.object_validation_ns)
    );
    println!(
        "{label}.object_validation_bytes={}",
        after
            .object_validation_bytes
            .saturating_sub(before.object_validation_bytes)
    );
    println!(
        "{label}.object_compare_ns={}",
        after
            .object_compare_ns
            .saturating_sub(before.object_compare_ns)
    );
    println!(
        "{label}.object_compare_bytes={}",
        after
            .object_compare_bytes
            .saturating_sub(before.object_compare_bytes)
    );
    println!(
        "{label}.object_auth_ns={}",
        after.object_auth_ns.saturating_sub(before.object_auth_ns)
    );
    println!(
        "{label}.index_lookups={}",
        after.index_lookups.saturating_sub(before.index_lookups)
    );
    println!(
        "{label}.index_lookup_ns={}",
        after.index_lookup_ns.saturating_sub(before.index_lookup_ns)
    );
    println!(
        "{label}.index_page_reads={}",
        after
            .index_page_reads
            .saturating_sub(before.index_page_reads)
    );
    println!(
        "{label}.index_cache_hits={}",
        after
            .index_cache_hits
            .saturating_sub(before.index_cache_hits)
    );
    println!(
        "{label}.index_cache_misses={}",
        after
            .index_cache_misses
            .saturating_sub(before.index_cache_misses)
    );
    println!(
        "{label}.index_cache_evictions={}",
        after
            .index_cache_evictions
            .saturating_sub(before.index_cache_evictions)
    );
    println!(
        "{label}.marker_capture_digest_ns={}",
        after
            .marker_capture_digest_ns
            .saturating_sub(before.marker_capture_digest_ns)
    );
    println!(
        "{label}.marker_sync_attempts={}",
        after
            .marker_sync_attempts
            .saturating_sub(before.marker_sync_attempts)
    );
    println!(
        "{label}.marker_sync_successes={}",
        after
            .marker_sync_successes
            .saturating_sub(before.marker_sync_successes)
    );
    println!(
        "{label}.marker_sync_failures={}",
        after
            .marker_sync_failures
            .saturating_sub(before.marker_sync_failures)
    );
    println!(
        "{label}.marker_sync_ns={}",
        after.marker_sync_ns.saturating_sub(before.marker_sync_ns)
    );
    println!(
        "{label}.writer_lock_wait_ns={}",
        after
            .writer_lock_wait_ns
            .saturating_sub(before.writer_lock_wait_ns)
    );
    println!(
        "{label}.writer_lock_hold_ns={}",
        after
            .writer_lock_hold_ns
            .saturating_sub(before.writer_lock_hold_ns)
    );
}

fn print_counters(label: &str, counters: &AppendOnlyCounters) {
    println!("{label}.frames_appended={}", counters.frames_appended);
    println!("{label}.frames_scanned={}", counters.frames_scanned);
    println!("{label}.frames_recovered={}", counters.frames_recovered);
    println!("{label}.objects_created={}", counters.objects_created);
    println!("{label}.objects_reused={}", counters.objects_reused);
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
    println!("{label}.carrier_flush_ns={}", counters.carrier_flush_ns);
    println!(
        "{label}.carrier_flush_failures={}",
        counters.carrier_flush_failures
    );
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
    println!(
        "{label}.marker_capture_digest_ns={}",
        counters.marker_capture_digest_ns
    );
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
    println!(
        "{label}.writer_lock_wait_ns={}",
        counters.writer_lock_wait_ns
    );
    println!(
        "{label}.writer_lock_hold_ns={}",
        counters.writer_lock_hold_ns
    );
    println!("{label}.reopen_scans={}", counters.reopen_scans);
    println!("{label}.residue_bytes={}", counters.residue_bytes);
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
}

fn empty_directory() -> (ObjectId, Vec<u8>) {
    let bytes = encode_object(&Object::directory(Vec::new()).expect("directory"))
        .expect("directory encoding");
    (ObjectId::for_bytes(&bytes), bytes)
}
