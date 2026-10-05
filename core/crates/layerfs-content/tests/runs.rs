mod support;
use layerfs_content::{
    construct_runs, construct_stream, read_range, ConstructionPolicy, ContentError, ContentResult,
    FileRun, FileRuns, FinalizedConsumer, FinalizedObject,
};
use std::{
    collections::VecDeque,
    io::{Cursor, Read},
};
use support::{disabled_scope, noise, MemoryStore, TrackingConsumer};

enum Part {
    Data(Cursor<Vec<u8>>),
    Zero(u64),
}
struct Runs {
    parts: VecDeque<Part>,
    window: usize,
}
impl Runs {
    fn mixed(prefix: usize, zero: u64, suffix: usize, window: usize) -> Self {
        let mut parts = VecDeque::new();
        if prefix != 0 {
            parts.push_back(Part::Data(Cursor::new(noise(prefix))));
        }
        if zero != 0 {
            parts.push_back(Part::Zero(zero));
        }
        if suffix != 0 {
            parts.push_back(Part::Data(Cursor::new(noise(suffix))));
        }
        Self { parts, window }
    }
}
impl FileRuns for Runs {
    fn read_run(&mut self, output: &mut [u8]) -> ContentResult<FileRun> {
        loop {
            match self.parts.front_mut() {
                Some(Part::Zero(n)) => {
                    let n = *n;
                    self.parts.pop_front();
                    return Ok(FileRun::Zero(n));
                }
                Some(Part::Data(data)) => {
                    let size = output.len().min(self.window);
                    let n = data.read(&mut output[..size]).unwrap();
                    if n != 0 {
                        return Ok(FileRun::Data(n));
                    }
                    self.parts.pop_front();
                }
                None => return Ok(FileRun::End),
            }
        }
    }
}

#[test]
fn run_roots_equal_streaming_at_cutoff_cdc_and_leaf_partition_boundaries() {
    let policy = ConstructionPolicy::frozen_default();
    for (prefix, zero, suffix, window) in [
        (0, 0, 0, 31),
        (7, 19, 11, 1),
        (0, 131071, 0, 32768),
        (0, 131072, 0, 32768),
        (0, 131073, 0, 32768),
        (131077, 32769, 9001, 113),
        (131083, 32768 * 130 + 7, 32873, 777),
        (32771, 32768 * 193 + 1, 3, 255),
        (131091, 32768 * 385, 10003, 32767),
    ] {
        let mut consumer = TrackingConsumer::new();
        let got = disabled_scope(|scope| {
            construct_runs(
                policy,
                &policy.capacities(),
                &mut Runs::mixed(prefix, zero, suffix, window),
                &mut consumer,
                scope.child("runs"),
            )
        })
        .unwrap();
        let source = Cursor::new(noise(prefix))
            .chain(std::io::repeat(0).take(zero))
            .chain(Cursor::new(noise(suffix)));
        let mut ordinary = MemoryStore::new();
        let expected = disabled_scope(|scope| {
            construct_stream(
                policy,
                &policy.capacities(),
                source,
                &mut ordinary,
                scope.child("stream"),
            )
        })
        .unwrap();
        assert_eq!(
            got.file.root, expected.root,
            "prefix={prefix} zero={zero} suffix={suffix} window={window}"
        );
        assert_eq!(got.file.logical_len, prefix as u64 + zero + suffix as u64);
        consumer.assert_children_precede_parents();
        println!("ZERO_EQUAL prefix={prefix} zero={zero} suffix={suffix} window={window} processed={} chunks={} nodes={}",
            got.zeros_processed, got.chunks_emitted, got.nodes_emitted);
    }
}

#[test]
fn multi_level_repetition_matches_streaming_extent_builder_without_zero_byte_scan() {
    use layerfs_content::file::mapping::ExtentBuilder;
    let capacities = ConstructionPolicy::frozen_default().capacities();
    let raw = [0; 32768];
    // Counts straddle the second-level flush and partition boundaries.
    for count in [24577, 24705, 49153, 65537] {
        let mut repeated = ExtentBuilder::new(&capacities);
        let mut actual = TrackingConsumer::new();
        repeated.push_chunk(b"prefix", None, &mut actual).unwrap();
        repeated
            .push_repeated_chunk(&raw, count, &mut actual)
            .unwrap();
        repeated.push_chunk(b"suffix", None, &mut actual).unwrap();
        let got = repeated.finish(&mut actual).unwrap();
        let mut ordinary = ExtentBuilder::new(&capacities);
        let mut expected = MemoryStore::new();
        ordinary.push_chunk(b"prefix", None, &mut expected).unwrap();
        // Keep the payload once; the public retained-slice path provides an
        // independent ordinary streaming oracle without hashing gigabytes.
        let id = ordinary.push_chunk(&raw, None, &mut expected).unwrap();
        for _ in 1..count {
            ordinary
                .push_extent(
                    layerfs_content::file::mapping::ExtentSlice::new(id, 0, 32768).unwrap(),
                    &mut expected,
                )
                .unwrap();
        }
        ordinary.push_chunk(b"suffix", None, &mut expected).unwrap();
        let want = ordinary.finish(&mut expected).unwrap();
        assert_eq!(got.root, want.root, "count={count}");
        assert_eq!(got.logical_len, want.logical_len);
        assert!(got.nodes < 20);
        actual.assert_children_precede_parents();
    }
}

#[test]
fn terabyte_zero_runs_emit_logarithmic_objects_and_read_exact_boundaries() {
    let policy = ConstructionPolicy::frozen_default();
    for zero in [1 << 20, 1 << 30, 1_u64 << 40] {
        let (prefix, suffix) = (131079, 413);
        let mut consumer = TrackingConsumer::new();
        let got = disabled_scope(|scope| {
            construct_runs(
                policy,
                &policy.capacities(),
                &mut Runs::mixed(prefix, zero, suffix, 997),
                &mut consumer,
                scope.child("runs"),
            )
        })
        .unwrap();
        assert!(got.zeros_processed <= 65536);
        assert!(got.nodes_emitted < 40);
        assert!(got.peak_pending <= 8 * policy.capacities().stream_flush_entries);
        for range in [
            prefix as u64 - 17..prefix as u64 + 31,
            prefix as u64 + zero / 2..prefix as u64 + zero / 2 + 19,
            prefix as u64 + zero - 11..prefix as u64 + zero + suffix as u64,
        ] {
            let mut bytes = Vec::new();
            disabled_scope(|scope| {
                read_range(
                    &consumer.store,
                    got.file.root,
                    range.clone(),
                    &mut bytes,
                    scope.child("read"),
                )
            })
            .unwrap();
            let prefix_data = noise(prefix);
            let suffix_data = noise(suffix);
            let expected: Vec<u8> = range
                .map(|at| {
                    if at < prefix as u64 {
                        prefix_data[at as usize]
                    } else if at < prefix as u64 + zero {
                        0
                    } else {
                        suffix_data[(at - prefix as u64 - zero) as usize]
                    }
                })
                .collect();
            assert_eq!(bytes, expected);
        }
        consumer.assert_children_precede_parents();
        println!("ZERO_SCALE logical_zero={zero} processed={} chunks={} nodes={} peak_pending={} canonical_bytes={}",
            got.zeros_processed, got.chunks_emitted, got.nodes_emitted, got.peak_pending, consumer.store.canonical_bytes());
    }
}

#[test]
fn invalid_windows_and_consumer_failure_stop_the_single_operation() {
    struct Invalid(FileRun);
    impl FileRuns for Invalid {
        fn read_run(&mut self, _: &mut [u8]) -> ContentResult<FileRun> {
            Ok(self.0)
        }
    }
    struct Refused(u64);
    impl FinalizedConsumer for Refused {
        fn accept(&mut self, _: FinalizedObject) -> ContentResult<()> {
            self.0 += 1;
            Err(ContentError::OutputRejected)
        }
    }
    let policy = ConstructionPolicy::frozen_default();
    for run in [FileRun::Data(0), FileRun::Data(32769), FileRun::Zero(0)] {
        let mut sink = Refused(0);
        let error = disabled_scope(|scope| {
            construct_runs(
                policy,
                &policy.capacities(),
                &mut Invalid(run),
                &mut sink,
                scope.child("runs"),
            )
        })
        .unwrap_err();
        assert!(matches!(error, ContentError::InvalidRecord(_)));
        assert_eq!(sink.0, 0);
    }
    let mut sink = Refused(0);
    let error = disabled_scope(|scope| {
        construct_runs(
            policy,
            &policy.capacities(),
            &mut Runs::mixed(0, 1 << 40, 0, 1),
            &mut sink,
            scope.child("runs"),
        )
    })
    .unwrap_err();
    assert!(matches!(error, ContentError::OutputRejected));
    assert_eq!(sink.0, 1);
}
