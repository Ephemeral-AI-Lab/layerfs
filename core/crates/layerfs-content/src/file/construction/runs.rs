//! Hole-aware complete construction through the ordinary canonical builder.
use crate::file::{
    cdc::MAXIMUM_CHUNK_BYTES,
    chunk_runs::ChunkRuns,
    construct_bytes_with_predecessor,
    mapping::{self, Aligner, PredecessorBase},
    ConstructedFile, FileContent,
};
use crate::{
    ConstructionCapacities, ConstructionPolicy, ContentError, ContentResult, FinalizedConsumer,
    ObjectId,
};
use layerfs_telemetry::timer::{Active, TimingScope};

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
    /// Data/Zero must be nonempty. Data must fit `output`; Zero is a logical
    /// span and may exceed it.
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
    construct_runs_with_predecessor(policy, capacities, source, None, consumer, scope)
}

/// Constructs exactly the root `construct_runs` does, offering a stored earlier
/// version of the file as the delta-base correspondence of its new chunks.
///
/// `base` is a file this one replaced, such as the target of a rename that put
/// it in place or the file the base tree held at the same path. Each new chunk
/// is offered the base payload its byte range corresponds to, through the same
/// alignment an edit uses, re-anchored at every chunk equal to a base payload.
/// For a chunked result the base is read: its root once, then its mapping pages
/// as the alignment walks forward, never a payload, and a whole-file base offers
/// no chunk anything. A whole-file result is offered the base root as its
/// explicit predecessor without any read (#88), as `construct_bytes_with_predecessor`
/// does. The hints are advisory, so the constructed root is byte for byte the one
/// `construct_runs` returns; with no base this is `construct_runs`.
pub fn construct_runs_with_predecessor(
    policy: ConstructionPolicy,
    capacities: &ConstructionCapacities,
    source: &mut dyn FileRuns,
    base: Option<PredecessorBase<'_>>,
    consumer: &mut dyn FinalizedConsumer,
    scope: TimingScope<'_>,
) -> ContentResult<RunConstruction> {
    policy.validated()?;
    let offered = base;
    scope.run(|active| {
        let mut hints = Hints {
            base,
            aligner: None,
            state: None,
            active,
        };
        let cutoff = policy.small_file_threshold_bytes() as usize;
        let mut prefix = Vec::with_capacity(cutoff);
        let mut input = [0; MAXIMUM_CHUNK_BYTES];
        let mut chunks = ChunkRuns::new(capacities);
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
                    // The result is chunked: its mapping pages are offered the
                    // base's pages at the same positions (#92), collected once.
                    if let Some(pages) = hints.page_hints()? {
                        chunks.set_page_hints(pages);
                    }
                    chunks.data(&prefix, &mut |s, l, i| hints.offer(s, l, i), consumer)?;
                    prefix.clear();
                    chunked = true;
                }
            }
            if chunked && used < length {
                if zero {
                    let processed =
                        chunks.zero(length - used, &mut |s, l, i| hints.offer(s, l, i), consumer)?;
                    zeros_processed = add(zeros_processed, processed)?;
                } else {
                    chunks.data(
                        &input[used as usize..length as usize],
                        &mut |s, l, i| hints.offer(s, l, i),
                        consumer,
                    )?;
                }
            }
        }
        if !chunked {
            // A small result takes the earlier version whole as its base.
            let file = construct_bytes_with_predecessor(
                policy,
                capacities,
                &prefix,
                offered,
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
        let build = chunks.finish(&mut |s, l, i| hints.offer(s, l, i), consumer)?;
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

/// The base a run construction offers its chunks, opened at the first chunk.
struct Hints<'b, 's, 'r> {
    base: Option<PredecessorBase<'b>>,
    aligner: Option<Aligner>,
    state: Option<crate::file::mapping::FileState>,
    active: &'s TimingScope<'r, Active>,
}
impl Hints<'_, '_, '_> {
    fn offer(&mut self, start: u64, len: u64, id: ObjectId) -> ContentResult<Option<ObjectId>> {
        if !self.open()? {
            return Ok(None);
        }
        let (Some(base), Some(aligner)) = (self.base, self.aligner.as_mut()) else {
            return Ok(None);
        };
        let active = self.active;
        let read = &mut |page: ObjectId, _root: bool| {
            base.reader()
                .read_canonical_scoped(page, active.child("mapping.predecessor"))
        };
        aligner.offer(start, len, id, read)
    }

    /// Opens the base once: true when it is chunked. A whole-file base offers
    /// nothing, and is not read again.
    fn open(&mut self) -> ContentResult<bool> {
        let Some(base) = self.base else {
            return Ok(false);
        };
        if self.aligner.is_some() {
            return Ok(true);
        }
        let canonical = base
            .reader()
            .read_canonical_scoped(base.root(), self.active.child("mapping.predecessor"))?;
        match crate::file::classify(&canonical)? {
            FileContent::Chunked(state) => {
                self.state = Some(state);
                self.aligner = Some(Aligner::new(state));
                Ok(true)
            }
            FileContent::WholeFile { .. } => {
                self.base = None;
                Ok(false)
            }
        }
    }

    /// The chunked base's branch levels for positional page hints (#92), or
    /// `None` for no base or a whole-file one.
    fn page_hints(&mut self) -> ContentResult<Option<crate::file::mapping::PageHints>> {
        if !self.open()? {
            return Ok(None);
        }
        let (Some(base), Some(state)) = (self.base, self.state) else {
            return Ok(None);
        };
        let active = self.active;
        crate::file::mapping::PageHints::collect(&state, &mut |page, _| {
            base.reader()
                .read_canonical_scoped(page, active.child("mapping.predecessor"))
        })
        .map(Some)
    }
}

fn add(a: u64, b: u64) -> ContentResult<u64> {
    a.checked_add(b).ok_or(ContentError::LengthOverflow)
}
