use super::super::artifact::maximum_rss_bytes;
use super::super::counter_validation::{engine_delta, verify_read_only_engine};
use super::super::environment::base;
use super::super::model::{BoundedRead, DigestSink, MIB};
use super::super::root_validation::{canonical_digest, expected_ref};
use super::super::summary_evidence::statistics;
use crate::legacy_full::IntegrityMode;
use crate::stage1_fixture::{
    expected_bytes, fixture_root, input_path, read_master, Attempt, BUFFER_BYTES, FILE_BYTES,
    FILE_PATH, RANDOM_RANGE_BYTES,
};
use std::fs::File;
use std::io::{Read, Write};
use std::time::Instant;
struct TimedWrite<W> {
    inner: W,
    wall_ns: u128,
    calls: u64,
    bytes: u64,
    maximum_write_bytes: usize,
}
impl<W> TimedWrite<W> {
    fn new(inner: W) -> Self {
        Self {
            inner,
            wall_ns: 0,
            calls: 0,
            bytes: 0,
            maximum_write_bytes: 0,
        }
    }
}
impl<W: Write> Write for TimedWrite<W> {
    fn write(&mut self, input: &[u8]) -> std::io::Result<usize> {
        let started = Instant::now();
        let written = self.inner.write(input)?;
        self.wall_ns = self.wall_ns.saturating_add(started.elapsed().as_nanos());
        self.calls += 1;
        self.bytes += written as u64;
        self.maximum_write_bytes = self.maximum_write_bytes.max(written);
        Ok(written)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}
#[cfg(target_os = "macos")]
#[test]
#[ignore = "requires the prepared 100 MiB APFS fixture"]
fn full_import_reports_the_phase_that_owns_peak_rss() {
    struct RssRead<R> {
        inner: R,
        bytes: u64,
        next_sample: u64,
        samples: std::rc::Rc<std::cell::RefCell<Vec<(u64, u64)>>>,
    }
    impl<R: Read> Read for RssRead<R> {
        fn read(&mut self, output: &mut [u8]) -> std::io::Result<usize> {
            let read = self.inner.read(output)?;
            self.bytes += read as u64;
            if self.bytes >= self.next_sample {
                self.samples
                    .borrow_mut()
                    .push((self.bytes, maximum_rss_bytes().unwrap()));
                self.next_sample += 10 * MIB;
            }
            Ok(read)
        }
    }
    let master = read_master(&fixture_root()).unwrap();
    let expected = base(&master, "import-genesis").unwrap();
    let baseline = maximum_rss_bytes().unwrap();
    let attempt = Attempt::create("import-genesis", expected).unwrap();
    let after_reset = maximum_rss_bytes().unwrap();
    let opened = attempt
        .open(expected, IntegrityMode::TrustedLocalDev)
        .unwrap();
    let after_open = maximum_rss_bytes().unwrap();
    let samples = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let input = BoundedRead(RssRead {
        inner: File::open(input_path(false)).unwrap(),
        bytes: 0,
        next_sample: 10 * MIB,
        samples: samples.clone(),
    });
    let (state, _) = opened
        .fs
        .replace_file_observed(&expected_ref(expected), FILE_PATH, input)
        .unwrap();
    let after_replace = maximum_rss_bytes().unwrap();
    assert_eq!(opened.fs.current_head("main").unwrap(), state);
    let after_head = maximum_rss_bytes().unwrap();
    let (bytes, digest, _) = canonical_digest(&opened.fs, state.root).unwrap();
    let after_digest = maximum_rss_bytes().unwrap();
    eprintln!(
            "peak_rss baseline={baseline} reset={after_reset} open={after_open} stream={:?} replace={after_replace} head={after_head} digest={after_digest}",
            samples.borrow()
        );
    assert_eq!((bytes, digest), (FILE_BYTES, master.raw_digest.clone()));
    assert!(
        after_digest <= 67_108_864,
        "full import exceeded 64 MiB RSS"
    );
    drop(opened);
    attempt.cleanup().unwrap();
}
#[cfg(target_os = "macos")]
#[test]
#[ignore = "requires the prepared 100 MiB APFS fixture"]
fn random_ranges_reuse_the_exact_file_read_plan() {
    let master = read_master(&fixture_root()).unwrap();
    let expected = base(&master, "read-reconstruct").unwrap();
    let blocks = FILE_BYTES / RANDOM_RANGE_BYTES;
    let mut observations = Vec::with_capacity(300);
    let mut statements = 0;
    let mut fetched_rows = 0;
    let mut connection_mutex_wait_ns = 0;
    let mut trust_guard_ns = 0;
    let mut payload_query_ns = 0;
    let mut role_decode_ns = 0;
    let mut payload_callback_inclusive_ns = 0;
    let mut counter_merge_ns = 0;
    for batch in 0..3_u64 {
        let attempt = Attempt::create("read-reconstruct", expected).unwrap();
        let opened = attempt
            .open(expected, IntegrityMode::TrustedLocalDev)
            .unwrap();
        let before = opened.fs.counter_snapshot().unwrap();
        for within in 0..100_u64 {
            let offset = (((batch * 100 + within) * 521 + 0x51) % blocks) * RANDOM_RANGE_BYTES;
            let mut output = Vec::with_capacity(RANDOM_RANGE_BYTES as usize);
            let started = Instant::now();
            opened
                .fs
                .read_range(
                    expected.root,
                    FILE_PATH,
                    offset..offset + RANDOM_RANGE_BYTES,
                    &mut output,
                )
                .unwrap();
            observations.push(started.elapsed().as_nanos());
            assert_eq!(
                output,
                expected_bytes(offset, RANDOM_RANGE_BYTES as usize).unwrap()
            );
        }
        let after = opened.fs.diagnostics().unwrap();
        statements += after.statements - before.statements;
        fetched_rows += after.fetched_rows - before.fetched_rows;
        connection_mutex_wait_ns +=
            after.connection_mutex_wait_ns - before.connection_mutex_wait_ns;
        trust_guard_ns += after.trust_guard_ns - before.trust_guard_ns;
        payload_query_ns += after.payload_query_ns - before.payload_query_ns;
        role_decode_ns += after.role_decode_ns - before.role_decode_ns;
        payload_callback_inclusive_ns +=
            after.payload_callback_inclusive_ns - before.payload_callback_inclusive_ns;
        counter_merge_ns += after.counter_merge_ns - before.counter_merge_ns;
        drop(opened);
        attempt.cleanup().unwrap();
    }
    let observed = statistics(&observations).unwrap();
    let mut sorted = observations.clone();
    sorted.sort_unstable();
    eprintln!(
        "A02 focused p50_ns={} p95_ns={} statements={} fetched_rows={} connection_mutex_wait_ns={} trust_guard_ns={} payload_query_ns={} role_decode_ns={} payload_callback_inclusive_ns={} counter_merge_ns={} raw_sorted_ns={sorted:?}",
        observed.p50,
        observed.p95,
        statements,
        fetched_rows,
        connection_mutex_wait_ns,
        trust_guard_ns,
        payload_query_ns,
        role_decode_ns,
        payload_callback_inclusive_ns,
        counter_merge_ns,
    );
    assert_eq!(statements, 635);
    assert_eq!(fetched_rows, 1632);
}
#[cfg(target_os = "macos")]
#[test]
#[ignore = "requires the prepared 100 MiB APFS fixture"]
fn large_authenticated_read_and_materialization_attribution() {
    let master = read_master(&fixture_root()).unwrap();
    let expected = base(&master, "read-reconstruct").unwrap();
    let attempt = Attempt::create("read-reconstruct", expected).unwrap();
    let opened = attempt
        .open(expected, IntegrityMode::TrustedLocalDev)
        .unwrap();
    let before = opened.fs.counter_snapshot().unwrap();
    let mut cold = TimedWrite::new(DigestSink::default());
    let started = Instant::now();
    let cold_operation = opened
        .fs
        .read_to(expected.root, FILE_PATH, &mut cold)
        .unwrap();
    let cold_wall_ns = started.elapsed().as_nanos();
    let after = opened.fs.counter_snapshot().unwrap();
    let cold_engine = engine_delta(&before, &after).unwrap();
    verify_read_only_engine(&cold_engine).unwrap();
    let before = after;
    let mut warm = TimedWrite::new(DigestSink::default());
    let started = Instant::now();
    let warm_operation = opened
        .fs
        .read_to(expected.root, FILE_PATH, &mut warm)
        .unwrap();
    let warm_wall_ns = started.elapsed().as_nanos();
    let after = opened.fs.counter_snapshot().unwrap();
    let warm_engine = engine_delta(&before, &after).unwrap();
    verify_read_only_engine(&warm_engine).unwrap();
    let before = after;
    let mut null = TimedWrite::new(std::io::sink());
    let started = Instant::now();
    let null_operation = opened
        .fs
        .read_to(expected.root, FILE_PATH, &mut null)
        .unwrap();
    let null_wall_ns = started.elapsed().as_nanos();
    let after = opened.fs.counter_snapshot().unwrap();
    let null_engine = engine_delta(&before, &after).unwrap();
    verify_read_only_engine(&null_engine).unwrap();
    let started = Instant::now();
    let (mut materialized, materialize_operation) = opened
        .fs
        .materialize_managed_observed(expected.root)
        .unwrap();
    let materialize_wall_ns = started.elapsed().as_nanos();
    let mut physical = TimedWrite::new(DigestSink::default());
    materialized.read_to(FILE_PATH, &mut physical).unwrap();
    let (cold_bytes, cold_digest) = cold.inner.finish();
    let (warm_bytes, warm_digest) = warm.inner.finish();
    let (physical_bytes, physical_digest) = physical.inner.finish();
    assert_eq!(
        (cold_bytes, cold_digest.as_str()),
        (FILE_BYTES, master.raw_digest.as_str())
    );
    assert_eq!(
        (warm_bytes, warm_digest.as_str()),
        (FILE_BYTES, master.raw_digest.as_str())
    );
    assert_eq!(
        (physical_bytes, physical_digest.as_str()),
        (FILE_BYTES, master.raw_digest.as_str())
    );
    assert_eq!(null.bytes, FILE_BYTES);
    assert!(cold_operation.namespace.nodes_read > 0);
    assert!(cold_operation.inode_table.nodes_read > 0);
    assert_eq!(warm_operation.namespace.nodes_read, 0);
    assert_eq!(warm_operation.inode_table.nodes_read, 0);
    assert_eq!(null_operation.namespace.nodes_read, 0);
    assert_eq!(null_operation.inode_table.nodes_read, 0);
    assert_eq!(
        cold_engine.fetched_rows,
        cold_engine.fetched_row_authentication_passes
    );
    assert_eq!(
        warm_engine.fetched_rows,
        warm_engine.fetched_row_authentication_passes
    );
    assert_eq!(
        null_engine.fetched_rows,
        null_engine.fetched_row_authentication_passes
    );
    assert_eq!(cold_engine.payload_batch_session_maximum, 64);
    assert_eq!(warm_engine.payload_batch_session_maximum, 64);
    assert_eq!(null_engine.payload_batch_session_maximum, 64);
    assert_eq!(cold_engine.payload_batch_references, 5_284);
    assert_eq!(warm_engine.payload_batch_references, 5_284);
    assert_eq!(null_engine.payload_batch_references, 5_284);
    assert_eq!(cold_engine.payload_batch_queries, 83);
    assert_eq!(warm_engine.payload_batch_queries, 83);
    assert_eq!(null_engine.payload_batch_queries, 83);
    assert!(cold.maximum_write_bytes <= BUFFER_BYTES);
    assert!(warm.maximum_write_bytes <= BUFFER_BYTES);
    assert!(null.maximum_write_bytes <= BUFFER_BYTES);
    assert_eq!(materialize_operation.workspace_materializations, 1);
    assert_eq!(materialize_operation.operation_q_terminal_bytes, 0);
    eprintln!(
        concat!(
            "large_read_attribution ",
            "cold_ns={} cold_sink_ns={} cold_statements={} cold_rows={} ",
            "warm_ns={} warm_sink_ns={} warm_statements={} warm_rows={} ",
            "null_ns={} null_sink_ns={} null_statements={} null_rows={} ",
            "payload_batches={}/{}/{} payload_refs={}/{}/{} ",
            "rope_nodes={}/{}/{} sink_calls={}/{}/{} max_write={}/{}/{} ",
            "materialize_ns={} materialize_rope_nodes={}"
        ),
        cold_wall_ns,
        cold.wall_ns,
        cold_engine.statements,
        cold_engine.fetched_rows,
        warm_wall_ns,
        warm.wall_ns,
        warm_engine.statements,
        warm_engine.fetched_rows,
        null_wall_ns,
        null.wall_ns,
        null_engine.statements,
        null_engine.fetched_rows,
        cold_engine.payload_batch_queries,
        warm_engine.payload_batch_queries,
        null_engine.payload_batch_queries,
        cold_engine.payload_batch_references,
        warm_engine.payload_batch_references,
        null_engine.payload_batch_references,
        cold_operation.rope.nodes_read,
        warm_operation.rope.nodes_read,
        null_operation.rope.nodes_read,
        cold.calls,
        warm.calls,
        null.calls,
        cold.maximum_write_bytes,
        warm.maximum_write_bytes,
        null.maximum_write_bytes,
        materialize_wall_ns,
        materialize_operation.rope.nodes_read,
    );
    materialized.discard().unwrap();
    drop(opened);
    attempt.cleanup().unwrap();
}
