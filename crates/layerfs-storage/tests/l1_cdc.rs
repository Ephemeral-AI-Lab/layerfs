#[path = "support/mod.rs"]
mod support;

use layerfs_storage::cdc::{
    BorrowedChunkV1, BoundaryConsumerV1, CdcBoundaryConsumerErrorV1, CdcControlV1,
    CdcSourceErrorV1, ChunkBoundaryV1, FastCdcV1, FastCdcV1Stream, MAXIMUM_CHUNK_BYTES,
};
use layerfs_storage::CoreError;
use support::{expected, fastcdc_golden_input, sha256};

#[derive(Default)]
struct Control {
    cancelled: bool,
    deadline: bool,
    cancellation_calls: usize,
    deadline_calls: usize,
}

impl CdcControlV1 for Control {
    fn cancellation_requested(&mut self) -> bool {
        self.cancellation_calls += 1;
        self.cancelled
    }

    fn deadline_exceeded(&mut self) -> bool {
        self.deadline_calls += 1;
        self.deadline
    }
}

#[derive(Default)]
struct RecordingConsumer {
    calls: usize,
    refuse_call: Option<usize>,
    ends: Vec<u64>,
    ranges: Vec<(u64, u64)>,
    bytes: Vec<u8>,
    saw_wrap: bool,
}

impl BoundaryConsumerV1 for RecordingConsumer {
    fn accept(
        &mut self,
        boundary: ChunkBoundaryV1,
        chunk: BorrowedChunkV1<'_>,
    ) -> Result<(), CdcBoundaryConsumerErrorV1> {
        self.calls += 1;
        if self.refuse_call == Some(self.calls) {
            return Err(CdcBoundaryConsumerErrorV1::Refused);
        }
        assert_eq!(boundary.len(), chunk.len() as u64);
        assert!(boundary.start() < boundary.end());
        assert!(chunk.len() <= MAXIMUM_CHUNK_BYTES);
        self.ends.push(boundary.end());
        self.ranges.push((boundary.start(), boundary.end()));
        self.bytes.extend_from_slice(chunk.first());
        self.bytes.extend_from_slice(chunk.second());
        self.saw_wrap |= !chunk.second().is_empty();
        Ok(())
    }
}

#[derive(Default)]
struct PauseAfterFirstBoundary {
    inner: RecordingConsumer,
}

impl BoundaryConsumerV1 for PauseAfterFirstBoundary {
    fn accept(
        &mut self,
        boundary: ChunkBoundaryV1,
        chunk: BorrowedChunkV1<'_>,
    ) -> Result<(), CdcBoundaryConsumerErrorV1> {
        self.inner.accept(boundary, chunk)
    }

    fn pause_after_accepted_boundary(&self) -> bool {
        self.inner.calls == 1
    }
}

fn one_shot(source: &[u8]) -> Vec<u64> {
    let chunker = FastCdcV1::new();
    let mut ends = Vec::new();
    let mut offset = 0;
    while offset < source.len() {
        let amount = chunker.cut(&source[offset..]).expect("exact cut");
        assert!((1..=MAXIMUM_CHUNK_BYTES).contains(&amount));
        offset += amount;
        ends.push(offset as u64);
    }
    ends
}

fn run_fragments(source: &[u8], pattern: &[usize], interleave_empty: bool) -> RecordingConsumer {
    let mut ring = [0_u8; MAXIMUM_CHUNK_BYTES];
    let mut control = Control::default();
    let mut consumer = RecordingConsumer::default();
    let mut stream = FastCdcV1::new()
        .stream(&mut ring, &mut control)
        .expect("exact ring");
    let mut offset = 0;
    let mut index = 0;
    while offset < source.len() {
        if interleave_empty {
            stream
                .push(Ok(&[]), &mut control, &mut consumer)
                .expect("empty fragment");
        }
        let requested = pattern[index % pattern.len()];
        let end = (offset + requested).min(source.len());
        stream
            .push(Ok(&source[offset..end]), &mut control, &mut consumer)
            .expect("positive fragment");
        offset = end;
        index += 1;
    }
    stream
        .finish(&mut control, &mut consumer)
        .expect("finish stream");
    consumer
}

fn assert_coverage(ranges: &[(u64, u64)], source_len: usize) {
    let mut expected_start = 0;
    for &(start, end) in ranges {
        assert_eq!(start, expected_start);
        assert!(start < end);
        assert!(end - start <= MAXIMUM_CHUNK_BYTES as u64);
        expected_start = end;
    }
    assert_eq!(expected_start, source_len as u64);
}

#[test]
fn exact_golden_boundaries_and_hostile_eof_edges_match() {
    let generated_32k = fastcdc_golden_input(32_768);
    let generated_100k = fastcdc_golden_input(100_000);
    assert_eq!(
        sha256(&generated_32k),
        expected("9d3dbe8a478f75fc9e66754267da822d5f7b20ece70bfdf03953d92a8c427363")
    );
    assert_eq!(
        sha256(&generated_100k),
        expected("ae185ec52770d5c67076421abd6c5579afb2598327824db5df6b6e9bbc5c96de")
    );
    assert_eq!(one_shot(&generated_32k), [16_688, 32_768]);
    assert_eq!(
        one_shot(&generated_100k),
        [16_688, 34_949, 52_688, 70_914, 90_807, 100_000]
    );
    for (len, expected_ends) in [
        (0, vec![]),
        (1, vec![1]),
        (8_191, vec![8_191]),
        (8_192, vec![8_192]),
        (8_193, vec![8_193]),
        (16_383, vec![16_383]),
        (16_384, vec![16_384]),
        (16_385, vec![16_385]),
        (32_767, vec![32_767]),
        (32_768, vec![32_768]),
        (32_769, vec![32_768, 32_769]),
        (65_537, vec![32_768, 65_536, 65_537]),
    ] {
        assert_eq!(one_shot(&vec![0; len]), expected_ends, "zero length {len}");
    }
}

#[test]
fn seventeen_fragmentation_schedules_equal_one_shot_and_exercise_wrap() {
    const ODD_PRIMES: &[usize] = &[1, 3, 5, 7, 11, 13, 17, 19, 23, 29, 31];
    const EDGE_CYCLE: &[usize] = &[
        1, 8_191, 8_192, 8_193, 16_383, 16_384, 16_385, 32_767, 32_768,
    ];
    let schedules: [(&[usize], bool); 17] = [
        (&[100_000], false),
        (&[1], false),
        (&[2], false),
        (ODD_PRIMES, false),
        (&[8_191], false),
        (&[8_192], false),
        (&[8_193], false),
        (&[16_383], false),
        (&[16_384], false),
        (&[16_385], false),
        (&[32_767], false),
        (&[32_768], false),
        (&[32_767, 1], false),
        (&[1, 32_767], false),
        (EDGE_CYCLE, false),
        (&[8_191], true),
        (&[31_337, 997, 17], false),
    ];
    let source = fastcdc_golden_input(100_000);
    let expected_ends = one_shot(&source);
    let mut saw_wrap = false;
    for (pattern, interleave_empty) in schedules {
        let captured = run_fragments(&source, pattern, interleave_empty);
        assert_eq!(captured.ends, expected_ends);
        assert_eq!(captured.bytes, source);
        assert_coverage(&captured.ranges, source.len());
        saw_wrap |= captured.saw_wrap;
    }
    assert!(
        saw_wrap,
        "at least one schedule must exercise both ring spans"
    );
}

#[test]
fn lifecycle_errors_poison_terminally_and_finish_is_idempotent() {
    let mut invalid_ring = [0_u8; MAXIMUM_CHUNK_BYTES - 1];
    let mut control = Control::default();
    assert_eq!(
        FastCdcV1::new()
            .stream(&mut invalid_ring, &mut control)
            .err(),
        Some(CoreError::ResourceRefused)
    );

    let mut ring = [0_u8; MAXIMUM_CHUNK_BYTES];
    let mut control = Control::default();
    let mut consumer = RecordingConsumer::default();
    let mut stream = FastCdcV1::new().stream(&mut ring, &mut control).unwrap();
    stream
        .push(Ok(b"final suffix"), &mut control, &mut consumer)
        .unwrap();
    stream.finish(&mut control, &mut consumer).unwrap();
    assert_eq!(consumer.ends, [12]);
    let calls = consumer.calls;
    stream.finish(&mut control, &mut consumer).unwrap();
    assert_eq!(consumer.calls, calls, "repeated finish emits nothing");
    assert_eq!(
        stream.push(Ok(b"late"), &mut control, &mut consumer),
        Err(CoreError::TrailingBytes)
    );

    let mut ring = [0_u8; MAXIMUM_CHUNK_BYTES];
    let mut control = Control::default();
    let mut consumer = RecordingConsumer::default();
    let mut stream = FastCdcV1::new().stream(&mut ring, &mut control).unwrap();
    assert_eq!(
        stream.push(Err(CdcSourceErrorV1::Failure), &mut control, &mut consumer),
        Err(CoreError::SourceFailure)
    );
    control.cancelled = true;
    assert_eq!(
        stream.finish(&mut control, &mut consumer),
        Err(CoreError::SourceFailure)
    );

    let mut ring = [0_u8; MAXIMUM_CHUNK_BYTES];
    let mut control = Control::default();
    let mut consumer = RecordingConsumer {
        refuse_call: Some(1),
        ..RecordingConsumer::default()
    };
    let mut stream = FastCdcV1::new().stream(&mut ring, &mut control).unwrap();
    assert_eq!(
        stream.push(Ok(&[0; MAXIMUM_CHUNK_BYTES]), &mut control, &mut consumer),
        Err(CoreError::SinkRefused)
    );
    assert!(
        consumer.ends.is_empty(),
        "refused boundary is not published"
    );
    let calls = consumer.calls;
    assert_eq!(
        stream.finish(&mut control, &mut consumer),
        Err(CoreError::SinkRefused)
    );
    assert_eq!(consumer.calls, calls, "poisoned state does not retry");
}

#[test]
fn bounded_fragment_can_pause_at_an_accepted_boundary_without_extra_publication() {
    let source = fastcdc_golden_input(100_000);
    let first_end = one_shot(&source)[0] as usize;
    let mut ring = [0_u8; MAXIMUM_CHUNK_BYTES];
    let mut control = Control::default();
    let mut consumer = PauseAfterFirstBoundary::default();
    let mut stream = FastCdcV1::new().stream(&mut ring, &mut control).unwrap();
    let consumed = stream
        .push_until_consumer_pause(Ok(&source), &mut control, &mut consumer)
        .unwrap();
    assert_eq!(consumer.inner.calls, 1);
    assert_eq!(consumer.inner.ends, [first_end as u64]);
    assert_eq!(consumer.inner.bytes, source[..first_end]);
    assert!((first_end..=first_end + 2).contains(&consumed));
    assert!(consumed < source.len());
    stream.finish_at_accepted_boundary(&mut control).unwrap();
    stream.finish_at_accepted_boundary(&mut control).unwrap();
}

#[test]
fn cancellation_and_deadline_precede_source_and_are_terminal() {
    let mut invalid_ring = [0_u8; MAXIMUM_CHUNK_BYTES - 1];
    let mut control = Control {
        cancelled: true,
        deadline: true,
        ..Control::default()
    };
    assert_eq!(
        FastCdcV1::new()
            .stream(&mut invalid_ring, &mut control)
            .err(),
        Some(CoreError::Cancelled)
    );
    assert_eq!((control.cancellation_calls, control.deadline_calls), (1, 0));

    let mut ring = [0_u8; MAXIMUM_CHUNK_BYTES];
    let mut control = Control::default();
    let mut consumer = RecordingConsumer::default();
    let mut stream = FastCdcV1::new().stream(&mut ring, &mut control).unwrap();
    control.deadline = true;
    assert_eq!(
        stream.push(Err(CdcSourceErrorV1::Failure), &mut control, &mut consumer),
        Err(CoreError::Deadline)
    );
    control.deadline = false;
    assert_eq!(
        stream.push(Ok(b"ignored"), &mut control, &mut consumer),
        Err(CoreError::Deadline)
    );
    assert_eq!(consumer.calls, 0);
}

#[derive(Default)]
struct CountingConsumer {
    next_start: u64,
    chunks: u64,
    max_chunk: usize,
}

impl BoundaryConsumerV1 for CountingConsumer {
    fn accept(
        &mut self,
        boundary: ChunkBoundaryV1,
        chunk: BorrowedChunkV1<'_>,
    ) -> Result<(), CdcBoundaryConsumerErrorV1> {
        assert_eq!(boundary.start(), self.next_start);
        assert_eq!(boundary.len(), chunk.len() as u64);
        self.next_start = boundary.end();
        self.chunks += 1;
        self.max_chunk = self.max_chunk.max(chunk.len());
        Ok(())
    }
}

#[test]
fn long_stream_retains_only_ring_reference_and_scalar_state() {
    let state_bytes = core::mem::size_of::<FastCdcV1Stream<'static>>();
    assert!(state_bytes <= 128);
    let mut ring = [0_u8; MAXIMUM_CHUNK_BYTES];
    let mut control = Control::default();
    let mut consumer = CountingConsumer::default();
    let mut stream = FastCdcV1::new().stream(&mut ring, &mut control).unwrap();
    let mut fragment = [0_u8; 4_093];
    for (index, byte) in fragment.iter_mut().enumerate() {
        *byte = (index as u8).wrapping_mul(29).wrapping_add(17);
    }
    for _ in 0..4_100 {
        stream
            .push(Ok(&fragment), &mut control, &mut consumer)
            .unwrap();
    }
    stream.finish(&mut control, &mut consumer).unwrap();
    assert_eq!(consumer.next_start, 4_100 * fragment.len() as u64);
    assert!(consumer.chunks > 500);
    assert!(consumer.max_chunk <= MAXIMUM_CHUNK_BYTES);
    assert_eq!(
        state_bytes,
        core::mem::size_of::<FastCdcV1Stream<'static>>()
    );
}
