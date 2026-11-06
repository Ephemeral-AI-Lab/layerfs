//! One scanner/builder for complete and localized run-aware chunk construction.
use crate::file::{
    cdc::{CdcCounters, Scanner, MAXIMUM_CHUNK_BYTES},
    mapping::{ExtentBuilder, MappingBuild},
    FileRun, FileRuns,
};
use crate::{
    ConstructionCapacities, ContentError, ContentResult, FinalizedConsumer, ObjectId,
    PredecessorProvenance,
};

/// Chooses a scanned chunk's advisory delta base from its logical start in the
/// build, its length and its identity.
pub(crate) type Hint<'h> = &'h mut dyn FnMut(u64, u64, ObjectId) -> ContentResult<Option<ObjectId>>;

/// The hint that offers no base.
pub(crate) fn no_hint(_: u64, _: u64, _: ObjectId) -> ContentResult<Option<ObjectId>> {
    Ok(None)
}

pub(crate) struct ChunkRuns {
    scanner: Scanner,
    builder: ExtentBuilder,
    counters: CdcCounters,
    logical_len: u64,
}
impl ChunkRuns {
    pub(crate) fn new(capacities: &ConstructionCapacities) -> Self {
        Self {
            scanner: Scanner::new(),
            builder: ExtentBuilder::new(capacities),
            counters: CdcCounters::default(),
            logical_len: 0,
        }
    }
    /// Offers the mapping pages this construction emits positional delta bases
    /// from an earlier version of the file (#92).
    pub(crate) fn set_page_hints(&mut self, hints: crate::file::mapping::PageHints) {
        self.builder.set_page_hints(hints);
    }
    pub(crate) fn data(
        &mut self,
        bytes: &[u8],
        hint: Hint<'_>,
        consumer: &mut dyn FinalizedConsumer,
    ) -> ContentResult<()> {
        if bytes.is_empty() {
            return Ok(());
        }
        self.logical_len = self
            .logical_len
            .checked_add(bytes.len() as u64)
            .ok_or(ContentError::LengthOverflow)?;
        self.scanner.consume(
            bytes,
            &mut |chunk| {
                self.builder
                    .push_chunk_hinted(chunk, PredecessorProvenance::ReusedRange, hint, consumer)
                    .map(|_| ())
            },
            &mut self.counters,
        )
    }
    pub(crate) fn zero(
        &mut self,
        length: u64,
        hint: Hint<'_>,
        consumer: &mut dyn FinalizedConsumer,
    ) -> ContentResult<u64> {
        if length == 0 {
            return Err(ContentError::InvalidRecord("file run window"));
        }
        self.logical_len = self
            .logical_len
            .checked_add(length)
            .ok_or(ContentError::LengthOverflow)?;
        self.scanner
            .zeros(length, &mut self.counters, &mut |chunk, count| {
                self.builder
                    .push_repeated_chunk_hinted(
                        chunk,
                        count,
                        PredecessorProvenance::ReusedRange,
                        hint,
                        consumer,
                    )
                    .map(|_| ())
            })
    }
    pub(crate) fn finish(
        mut self,
        hint: Hint<'_>,
        consumer: &mut dyn FinalizedConsumer,
    ) -> ContentResult<MappingBuild> {
        self.scanner.finish(
            &mut |chunk| {
                self.builder
                    .push_chunk_hinted(chunk, PredecessorProvenance::ReusedRange, hint, consumer)
                    .map(|_| ())
            },
            &mut self.counters,
        )?;
        let build = self.builder.finish(consumer)?;
        if build.logical_len != self.logical_len {
            return Err(ContentError::LengthMismatch {
                expected: self.logical_len,
                actual: build.logical_len,
            });
        }
        Ok(build)
    }
}

pub(crate) fn build(
    capacities: &ConstructionCapacities,
    source: &mut dyn FileRuns,
    hint: Hint<'_>,
    consumer: &mut dyn FinalizedConsumer,
) -> ContentResult<MappingBuild> {
    let mut chunks = ChunkRuns::new(capacities);
    let mut input = [0; MAXIMUM_CHUNK_BYTES];
    loop {
        match source.read_run(&mut input)? {
            FileRun::Data(count) if count != 0 && count <= input.len() => {
                chunks.data(&input[..count], &mut *hint, consumer)?;
            }
            FileRun::Zero(length) if length != 0 => {
                chunks.zero(length, &mut *hint, consumer)?;
            }
            FileRun::End => return chunks.finish(hint, consumer),
            _ => return Err(ContentError::InvalidRecord("file run window")),
        }
    }
}
