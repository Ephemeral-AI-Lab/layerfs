//! Hole-aware complete construction through the ordinary canonical builder.
use super::{
    cdc::{CdcCounters, Scanner, MAXIMUM_CHUNK_BYTES},
    construct_bytes, mapping, ConstructedFile, ExtentBuilder,
};
use crate::{
    ConstructionCapacities, ConstructionPolicy, ContentError, ContentResult, FinalizedConsumer,
};
use layerfs_telemetry::timer::TimingScope;

/// One result of a bounded run-source read.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FileRun {
    /// Nonempty bytes placed in the caller's input buffer.
    Data(usize),
    /// This many logical zero bytes, without a payload allocation.
    Zero(u64),
    /// Definitive end of input.
    End,
}

/// Stable streaming input with explicit zero runs. Windows bound one read;
/// neither logical file size nor the total number of runs is capped.
pub trait FileRuns {
    /// Fills a prefix of `output`, declares a zero run, or ends the input.
    /// Zero-length data/zero runs and lengths exceeding `output` are invalid.
    /// The caller owns `output`; it may be reused after this call is consumed.
    fn read_run(&mut self, output: &mut [u8]) -> ContentResult<FileRun>;
}

/// Result plus actual work observations for one run constructor.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RunConstruction {
    /// Complete canonical file.
    pub file: ConstructedFile,
    /// Actual non-hole input bytes delivered by the source.
    pub data_bytes: u64,
    /// Logical zero bytes declared by the source.
    pub zero_bytes: u64,
    /// Zero bytes materialized by the threshold probe and scanner.
    pub zeros_processed: u64,
    /// Actual chunk payload objects accepted, including repeated runs once.
    pub chunks_emitted: u64,
    /// Actual mapping pages accepted.
    pub nodes_emitted: u64,
    /// Peak unfinished mapping entries; consumer custody is separate.
    pub peak_pending: usize,
}

/// Constructs exactly the root obtained by streaming the declared logical
/// bytes, reusing full zero subtrees instead of processing a hole's length.
/// Small files still materialize at most the policy's whole-file threshold.
/// Child-before-parent acceptance and consumer backpressure remain synchronous.
pub fn construct_runs(
    policy: ConstructionPolicy,
    capacities: &ConstructionCapacities,
    source: &mut dyn FileRuns,
    consumer: &mut dyn FinalizedConsumer,
    scope: TimingScope<'_>,
) -> ContentResult<RunConstruction> {
    policy.validated()?;
    scope.run(|active| {
        let cutoff = policy.small_file_threshold_bytes() as usize;
        let mut prefix = Vec::with_capacity(cutoff);
        let mut input = [0; MAXIMUM_CHUNK_BYTES];
        let mut builder = ExtentBuilder::new(capacities);
        let mut scanner = Scanner::new();
        let mut cdc = CdcCounters::default();
        let (mut data_bytes, mut zero_bytes, mut zeros_processed) = (0_u64, 0_u64, 0_u64);
        let mut chunked = false;
        loop {
            let run = source.read_run(&mut input)?;
            let (length, zero) = match run {
                FileRun::Data(n) if n != 0 && n <= input.len() => {
                    data_bytes = add(data_bytes, n as u64)?;
                    (n as u64, false)
                }
                FileRun::Zero(n) if n != 0 => {
                    zero_bytes = add(zero_bytes, n)?;
                    (n, true)
                }
                FileRun::End => break,
                _ => return Err(ContentError::InvalidRecord("file run window")),
            };
            let mut used = 0;
            if !chunked {
                used = length.min((cutoff - prefix.len()) as u64);
                if zero {
                    prefix.resize(prefix.len() + used as usize, 0);
                    zeros_processed = add(zeros_processed, used)?;
                } else {
                    prefix.extend_from_slice(&input[..used as usize]);
                }
                if prefix.len() == cutoff {
                    scanner.consume(
                        &prefix,
                        &mut |chunk| builder.push_chunk(chunk, None, consumer).map(|_| ()),
                        &mut cdc,
                    )?;
                    prefix.clear();
                    chunked = true;
                }
            }
            if chunked && used < length {
                if zero {
                    let processed =
                        scanner.zeros(length - used, &mut cdc, &mut |chunk, count| {
                            builder
                                .push_repeated_chunk(chunk, count, consumer)
                                .map(|_| ())
                        })?;
                    zeros_processed = add(zeros_processed, processed)?;
                } else {
                    scanner.consume(
                        &input[used as usize..length as usize],
                        &mut |chunk| builder.push_chunk(chunk, None, consumer).map(|_| ()),
                        &mut cdc,
                    )?;
                }
            }
        }
        if !chunked {
            let file = construct_bytes(
                policy,
                capacities,
                &prefix,
                consumer,
                active.child("content.runs.small"),
            )?;
            return Ok(RunConstruction {
                file,
                data_bytes,
                zero_bytes,
                zeros_processed,
                chunks_emitted: 0,
                nodes_emitted: u64::from(prefix.is_empty()),
                peak_pending: 0,
            });
        }
        scanner.finish(
            &mut |chunk| builder.push_chunk(chunk, None, consumer).map(|_| ()),
            &mut cdc,
        )?;
        let build = builder.finish(consumer)?;
        let summary = build
            .root
            .ok_or(ContentError::InvalidRecord("empty run mapping"))?;
        let root = mapping::emit_file_state(consumer, summary)?;
        let file = ConstructedFile {
            root,
            logical_len: summary.bytes,
            counters: Default::default(),
        };
        if file.logical_len != add(data_bytes, zero_bytes)? {
            return Err(ContentError::InvalidRecord("file run accounting"));
        }
        Ok(RunConstruction {
            file,
            data_bytes,
            zero_bytes,
            zeros_processed,
            chunks_emitted: build.chunks,
            nodes_emitted: build.nodes,
            peak_pending: build.peak_pending,
        })
    })
}

fn add(a: u64, b: u64) -> ContentResult<u64> {
    a.checked_add(b).ok_or(ContentError::LengthOverflow)
}
