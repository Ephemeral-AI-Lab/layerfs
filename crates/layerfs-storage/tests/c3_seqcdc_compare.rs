#![cfg(feature = "operation-polymorphism")]

#[path = "reference/naive_seqcdc.rs"]
mod naive_seqcdc;

use layerfs_storage::cdc::{
    BorrowedChunkV1, BoundaryConsumerV1, CdcBoundaryConsumerErrorV1, CdcControlV1,
    CdcSourceErrorV1, ChunkBoundaryV1, ContinueCdcControlV1, MAXIMUM_CHUNK_BYTES,
};
use layerfs_storage::cdc::{SeqCdcV1, SeqCdcV1Stream};
use layerfs_storage::CoreError;
use naive_seqcdc::OracleResult;

#[derive(Default)]
struct RecordingConsumer {
    calls: usize,
    refuse_call: Option<usize>,
    pause_call: Option<usize>,
    ends: Vec<u64>,
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
        self.ends.push(boundary.end());
        self.bytes.extend_from_slice(chunk.first());
        self.bytes.extend_from_slice(chunk.second());
        self.saw_wrap |= !chunk.second().is_empty();
        Ok(())
    }

    fn pause_after_accepted_boundary(&self) -> bool {
        self.pause_call == Some(self.calls)
    }
}

#[derive(Default)]
struct Control {
    cancelled: bool,
    deadline: bool,
}

impl CdcControlV1 for Control {
    fn cancellation_requested(&mut self) -> bool {
        self.cancelled
    }

    fn deadline_exceeded(&mut self) -> bool {
        self.deadline
    }
}

fn oracle_ends(source: &[u8]) -> Vec<u64> {
    let mut offset = 0;
    let mut ends = Vec::new();
    while offset < source.len() {
        let amount = naive_seqcdc::cut(&source[offset..]).cut;
        assert!((1..=MAXIMUM_CHUNK_BYTES).contains(&amount));
        offset += amount;
        ends.push(offset as u64);
    }
    ends
}

fn seqcdc_ends(source: &[u8]) -> Vec<u64> {
    let mut offset = 0;
    let mut ends = Vec::new();
    while offset < source.len() {
        let amount = SeqCdcV1::new().cut(&source[offset..]).expect("OS cut");
        offset += amount;
        ends.push(offset as u64);
    }
    ends
}

fn deterministic_bytes(len: usize, seed: u64) -> Vec<u8> {
    let mut state = seed;
    (0..len)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state as u8
        })
        .collect()
}

fn run_fragments(source: &[u8], pattern: &[usize], empty: bool) -> RecordingConsumer {
    let mut ring = [0_u8; MAXIMUM_CHUNK_BYTES];
    let mut control = ContinueCdcControlV1;
    let mut consumer = RecordingConsumer::default();
    let mut stream = SeqCdcV1::new().stream(&mut ring, &mut control).unwrap();
    feed(
        &mut stream,
        source,
        pattern,
        empty,
        &mut control,
        &mut consumer,
    );
    stream.finish(&mut control, &mut consumer).unwrap();
    consumer
}

fn feed(
    stream: &mut SeqCdcV1Stream<'_>,
    source: &[u8],
    pattern: &[usize],
    empty: bool,
    control: &mut ContinueCdcControlV1,
    consumer: &mut RecordingConsumer,
) {
    let mut offset = 0;
    let mut index = 0;
    while offset < source.len() {
        if empty {
            stream.push(Ok(&[]), control, consumer).unwrap();
        }
        let end = (offset + pattern[index % pattern.len()]).min(source.len());
        stream
            .push(Ok(&source[offset..end]), control, consumer)
            .unwrap();
        offset = end;
        index += 1;
    }
}

fn increasing_fixture(increases: usize) -> Vec<u8> {
    let mut source = vec![0_u8; naive_seqcdc::MINIMUM + increases + 1];
    for step in 1..=increases {
        source[naive_seqcdc::MINIMUM - 1 + step] = step as u8;
    }
    source
}

fn jump_fixture() -> Vec<u8> {
    let mut source = vec![0_u8; 9_000];
    source[naive_seqcdc::MINIMUM - 1] = 100;
    for step in 0..50 {
        source[naive_seqcdc::MINIMUM + step] = 99 - step as u8;
    }
    for step in 1..=5 {
        source[8_753 + step] = step as u8;
    }
    source
}

#[test]
fn pinned_hand_vectors_freeze_unsigned_equal_threshold_jump_clamp_and_eof() {
    let four = increasing_fixture(4);
    let five = increasing_fixture(5);
    let six = increasing_fixture(6);
    assert_eq!(naive_seqcdc::cut(&four).cut, four.len());
    assert_eq!(naive_seqcdc::cut(&five).cut, 8_196);
    assert_eq!(naive_seqcdc::cut(&six).cut, 8_196);

    let mut unsigned = vec![0_u8; 8_200];
    unsigned[8_191..8_197].copy_from_slice(&[0x7f, 0x80, 0x81, 0x82, 0xfe, 0xff]);
    assert_eq!(naive_seqcdc::cut(&unsigned).cut, 8_196);

    let mut equal = vec![0_u8; 8_205];
    equal[8_192..8_200].copy_from_slice(&[1, 1, 2, 2, 3, 3, 4, 5]);
    let equal_result = naive_seqcdc::cut(&equal);
    assert_eq!(equal_result.cut, 8_199);
    assert_eq!(equal_result.equal_absorptions, 3);

    let jump = jump_fixture();
    assert_eq!(
        naive_seqcdc::cut(&jump),
        OracleResult {
            cut: 8_758,
            comparisons: 55,
            equal_absorptions: 0,
            opposing_slopes: 50,
            jumps: 1,
            jump_bytes: 512,
        }
    );

    let mut forty_nine = vec![0_u8; 8_300];
    forty_nine[8_191] = 100;
    for step in 0..49 {
        forty_nine[8_192 + step] = 99 - step as u8;
    }
    for step in 1..=5 {
        forty_nine[8_240 + step] = 51 + step as u8;
    }
    assert_eq!(naive_seqcdc::cut(&forty_nine).jumps, 0);

    let mut delayed_fiftieth = forty_nine.clone();
    delayed_fiftieth[8_241] = 52;
    delayed_fiftieth[8_242] = 51;
    assert_eq!(naive_seqcdc::cut(&delayed_fiftieth).jumps, 1);

    let mut jump_past_eof = jump_fixture();
    jump_past_eof.truncate(8_400);
    assert_eq!(naive_seqcdc::cut(&jump_past_eof).cut, 8_400);
    assert_eq!(naive_seqcdc::cut(&jump_past_eof).jumps, 1);

    assert_eq!(naive_seqcdc::cut(&vec![0; 40_000]).cut, 32_768);

    for source in [four, five, six, unsigned, equal, jump, jump_past_eof] {
        assert_eq!(seqcdc_ends(&source), oracle_ends(&source));
    }
}

#[test]
fn optimized_seqcdc_matches_oracle_on_hostile_corpora() {
    let lengths = [
        0, 1, 8_191, 8_192, 8_193, 16_383, 16_384, 16_385, 32_767, 32_768, 32_769, 65_537, 100_000,
        262_147, 1_048_579,
    ];
    for len in lengths {
        for (name, source) in [
            ("zero", vec![0; len]),
            ("ones", vec![0xff; len]),
            ("seed-1", deterministic_bytes(len, 1)),
            (
                "seed-prime",
                deterministic_bytes(len, 0x9e37_79b9_7f4a_7c15),
            ),
        ] {
            assert_eq!(
                seqcdc_ends(&source),
                oracle_ends(&source),
                "{name} length {len}"
            );
        }
    }
}

#[test]
fn optimized_seqcdc_fragmentation_is_oracle_exact_and_wraps() {
    const PRIMES: &[usize] = &[1, 3, 5, 7, 11, 13, 17, 19, 23, 29, 31];
    const EDGES: &[usize] = &[
        1, 8_191, 8_192, 8_193, 16_383, 16_384, 16_385, 32_767, 32_768,
    ];
    let schedules: [(&[usize], bool); 17] = [
        (&[262_147], false),
        (&[1], false),
        (&[4_096], false),
        (PRIMES, false),
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
        (EDGES, false),
        (&[8_191], true),
        (&[31_337, 997, 17], false),
    ];
    let source = deterministic_bytes(262_147, 0xd1b5_4a32_d192_ed03);
    let expected = oracle_ends(&source);
    let mut saw_wrap = false;
    for (pattern, empty) in schedules {
        let captured = run_fragments(&source, pattern, empty);
        assert_eq!(captured.ends, expected, "schedule {pattern:?}");
        assert_eq!(captured.bytes, source);
        saw_wrap |= captured.saw_wrap;
    }
    assert!(saw_wrap);
}

#[test]
fn optimized_seqcdc_pause_counters_and_terminal_errors_are_exact() {
    let jump = jump_fixture();
    let mut ring = [0_u8; MAXIMUM_CHUNK_BYTES];
    let mut control = ContinueCdcControlV1;
    let mut consumer = RecordingConsumer::default();
    let mut stream = SeqCdcV1::new().stream(&mut ring, &mut control).unwrap();
    stream.push(Ok(&jump), &mut control, &mut consumer).unwrap();
    stream.finish(&mut control, &mut consumer).unwrap();
    assert_eq!(stream.seqcdc_counters().comparisons, 55);
    assert_eq!(stream.seqcdc_counters().opposing_slopes, 50);
    assert_eq!(stream.seqcdc_counters().jumps, 1);
    assert_eq!(stream.seqcdc_counters().jump_bytes, 512);
    assert_eq!(stream.counters().boundary_inspected_bytes, 110);

    let source = deterministic_bytes(100_000, 7);
    let first_boundary = oracle_ends(&source)[0] as usize;
    let mut ring = [0_u8; MAXIMUM_CHUNK_BYTES];
    let mut control = ContinueCdcControlV1;
    let mut consumer = RecordingConsumer {
        pause_call: Some(1),
        ..RecordingConsumer::default()
    };
    let mut stream = SeqCdcV1::new().stream(&mut ring, &mut control).unwrap();
    let consumed = stream
        .push_until_consumer_pause(Ok(&source), &mut control, &mut consumer)
        .unwrap();
    assert_eq!(consumer.ends, [first_boundary as u64]);
    assert_eq!(consumer.bytes, source[..first_boundary]);
    assert_eq!(consumed, first_boundary + 1);
    stream.finish_at_accepted_boundary(&mut control).unwrap();

    let mut ring = [0_u8; MAXIMUM_CHUNK_BYTES];
    let mut control = Control::default();
    let mut consumer = RecordingConsumer::default();
    let mut stream = SeqCdcV1::new().stream(&mut ring, &mut control).unwrap();
    assert_eq!(
        stream.push(Err(CdcSourceErrorV1::Failure), &mut control, &mut consumer),
        Err(CoreError::SourceFailure)
    );
    control.deadline = true;
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
    let mut stream = SeqCdcV1::new().stream(&mut ring, &mut control).unwrap();
    assert_eq!(
        stream.push(Ok(&source), &mut control, &mut consumer),
        Err(CoreError::SinkRefused)
    );
    assert_eq!(
        stream.push(Ok(b"later"), &mut control, &mut consumer),
        Err(CoreError::SinkRefused)
    );
}
