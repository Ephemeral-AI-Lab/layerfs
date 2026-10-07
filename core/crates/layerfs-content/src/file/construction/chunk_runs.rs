//! One scanner/builder for complete and localized run-aware chunk construction.
use crate::file::{
    cdc::{CdcCounters, Scanner, MAXIMUM_CHUNK_BYTES},
    mapping::{ExtentBuilder, MappingBuild},
    FileRun, FileRuns,
};
use crate::{ConstructionCapacities, ContentError, ContentResult, FinalizedConsumer, ObjectId};

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
    pub(crate) fn data(
        &mut self,
        bytes: &[u8],
        predecessor: Option<ObjectId>,
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
                    .push_chunk(chunk, predecessor, consumer)
                    .map(|_| ())
            },
            &mut self.counters,
        )
    }
    pub(crate) fn zero(
        &mut self,
        length: u64,
        predecessor: Option<ObjectId>,
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
                    .push_repeated_chunk_with_predecessor(chunk, count, predecessor, consumer)
                    .map(|_| ())
            })
    }
    pub(crate) fn finish(
        mut self,
        predecessor: Option<ObjectId>,
        consumer: &mut dyn FinalizedConsumer,
    ) -> ContentResult<MappingBuild> {
        self.scanner.finish(
            &mut |chunk| {
                self.builder
                    .push_chunk(chunk, predecessor, consumer)
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
    predecessor: Option<ObjectId>,
    consumer: &mut dyn FinalizedConsumer,
) -> ContentResult<MappingBuild> {
    let mut chunks = ChunkRuns::new(capacities);
    let mut input = [0; MAXIMUM_CHUNK_BYTES];
    loop {
        match source.read_run(&mut input)? {
            FileRun::Data(count) if count != 0 && count <= input.len() => {
                chunks.data(&input[..count], predecessor, consumer)?;
            }
            FileRun::Zero(length) if length != 0 => {
                chunks.zero(length, predecessor, consumer)?;
            }
            FileRun::End => return chunks.finish(predecessor, consumer),
            _ => return Err(ContentError::InvalidRecord("file run window")),
        }
    }
}
