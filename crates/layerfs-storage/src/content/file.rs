//! Canonical regular-file construction and bounded storage ports.
//!
//! This module owns one-pass file-object preparation. Complete root
//! orchestration stays in `create`, while Replace and Update reuse these exact
//! encoding and accounting mechanics.

use crate::cdc::{
    BorrowedChunkV1, BoundaryConsumerV1, CdcBoundaryConsumerErrorV1, CdcControlV1,
    CdcSourceErrorV1, ChunkBoundaryV1, FastCdcV1, MAXIMUM_CHUNK_BYTES,
};
use crate::format::{
    validate_chunk_refs_per_file, validate_file_mode, validate_logical_length,
    PhysicalObjectKindV1, ValidatedPath,
};
use crate::identity::{
    derive_logical_chunk_spans_v1, FramedHasherV1, LogicalChunkIdV1, LogicalChunkRefV1,
    LogicalFileHasherV1, LogicalFileIdentityV1, PhysicalChunkIdV1, PhysicalFileIdV1,
    IDENTITY_HASHER_BYTES_V1, TAG_PHYSICAL_CHUNK, TAG_PHYSICAL_FILE,
};
#[cfg(feature = "operation-polymorphism")]
use crate::limits::OperationReservationV1;
#[cfg(test)]
use crate::limits::ResourceLedgerV1;
use crate::limits::{
    CounterFieldV1, MemoryComponentV1, ObservationScopeV1, OperationCountersV1,
    OperationMemoryPlanV1, OptionalU64ObservationV1,
};
use crate::object::{
    encode_physical_object_header_v1, TypedPhysicalObjectIdV1, OBJECT_HEADER_BYTES,
};
use crate::{CoreError, CoreResult};

const FILE_FIXED_PAYLOAD_BYTES: u64 = 14;
const DATA_EXTENT_FIXED_BYTES: u64 = 13;
const CHUNK_REFERENCE_BYTES: u64 = 36;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ContentSourceErrorV1 {
    Failure,
}

pub trait ContentSourceV1 {
    /// Maximum transient userspace memory retained by this adapter while the
    /// operation is active. This declaration must be side-effect-free: it may
    /// not read, allocate, cache, or otherwise consume the source.
    fn resident_memory_bound_bytes(&self) -> CoreResult<u64>;
    fn read(&mut self, destination: &mut [u8]) -> Result<usize, ContentSourceErrorV1>;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PreparedSinkErrorV1 {
    Refused,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ObjectDispositionV1 {
    Created,
    Reused,
}

/// Transaction-private canonical object output. No method grants authority or
/// publication; `finish_closure` freezes only a complete prepared result.
pub trait PreparedObjectSinkV1 {
    /// Maximum transient userspace memory retained by this adapter. Durable
    /// private-object carrier bytes are storage quota, not resident memory;
    /// every userspace cache/window must be declared here. The query itself
    /// must not allocate or begin a closure.
    fn resident_memory_bound_bytes(&self) -> CoreResult<u64>;
    fn begin_closure(&mut self) -> Result<(), PreparedSinkErrorV1>;
    fn begin_object(
        &mut self,
        kind: PhysicalObjectKindV1,
        complete_len: u64,
    ) -> Result<(), PreparedSinkErrorV1>;
    fn write_private(&mut self, bytes: &[u8]) -> Result<(), PreparedSinkErrorV1>;
    fn finish_object(
        &mut self,
        expected_id: TypedPhysicalObjectIdV1,
    ) -> Result<ObjectDispositionV1, PreparedSinkErrorV1>;
    fn finish_closure(&mut self, result: PreparedFileV1) -> Result<(), PreparedSinkErrorV1>;
    fn abort_closure(&mut self);
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PreparedChunkRefV1 {
    logical_id: LogicalChunkIdV1,
    physical_id: PhysicalChunkIdV1,
    len: u32,
}

impl PreparedChunkRefV1 {
    pub(crate) const fn from_parts(
        logical_id: LogicalChunkIdV1,
        physical_id: PhysicalChunkIdV1,
        len: u32,
    ) -> Self {
        Self {
            logical_id,
            physical_id,
            len,
        }
    }

    pub const fn logical_id(self) -> LogicalChunkIdV1 {
        self.logical_id
    }

    pub const fn physical_id(self) -> PhysicalChunkIdV1 {
        self.physical_id
    }

    #[allow(clippy::len_without_is_empty)]
    pub const fn len(self) -> u32 {
        self.len
    }

    pub(crate) fn logical_ref(self) -> LogicalChunkRefV1 {
        LogicalChunkRefV1::from_parts(self.logical_id, u64::from(self.len))
    }
}

/// Independently charged fixed-record metadata storage. Implementations may
/// use a bounded file/run; the core never assumes in-memory residency.
pub trait ChunkReferenceSpoolV1 {
    /// Maximum userspace bytes that this spool can keep resident while
    /// retaining `maximum_refs` records. The declaration is queried and
    /// charged before LayerFS consumes source bytes or asks the spool to
    /// allocate. Spill-file bytes are immutable-storage quota, not resident
    /// memory, but every userspace window/cache used by the implementation
    /// must be included here.
    fn resident_memory_bound_bytes(&self, maximum_refs: u64) -> CoreResult<u64>;
    /// Exact external spill bytes currently occupied when the implementation
    /// can report them directly. Unavailable observations carry a reason and
    /// no numeric value.
    fn storage_bytes_observation(&self) -> CoreResult<OptionalU64ObservationV1> {
        Ok(OptionalU64ObservationV1::unavailable(
            "chunk-reference port exposes no direct spill-byte observation",
            ObservationScopeV1::Operation,
        ))
    }
    fn begin(&mut self, maximum_refs: u64) -> Result<(), PreparedSinkErrorV1>;
    fn push(&mut self, chunk: PreparedChunkRefV1) -> Result<(), PreparedSinkErrorV1>;
    fn rewind(&mut self) -> Result<(), PreparedSinkErrorV1>;
    fn next(&mut self) -> Result<Option<PreparedChunkRefV1>, PreparedSinkErrorV1>;
    fn abort(&mut self);
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PreparedFileV1 {
    logical_file: LogicalFileIdentityV1,
    physical_file: PhysicalFileIdV1,
    chunk_count: u32,
}

impl PreparedFileV1 {
    pub(crate) const fn new(
        logical_file: LogicalFileIdentityV1,
        physical_file: PhysicalFileIdV1,
        chunk_count: u32,
    ) -> Self {
        Self {
            logical_file,
            physical_file,
            chunk_count,
        }
    }

    pub const fn logical_file(self) -> LogicalFileIdentityV1 {
        self.logical_file
    }

    pub const fn physical_file(self) -> PhysicalFileIdV1 {
        self.physical_file
    }

    pub const fn chunk_count(self) -> u32 {
        self.chunk_count
    }
}

pub struct ContentBuffersV1<'buffers> {
    pub(crate) source: &'buffers mut [u8; MAXIMUM_CHUNK_BYTES],
    pub(crate) cdc_ring: &'buffers mut [u8; MAXIMUM_CHUNK_BYTES],
}

impl<'buffers> ContentBuffersV1<'buffers> {
    pub fn new(
        source: &'buffers mut [u8; MAXIMUM_CHUNK_BYTES],
        cdc_ring: &'buffers mut [u8; MAXIMUM_CHUNK_BYTES],
    ) -> Self {
        Self { source, cdc_ring }
    }
}

#[allow(clippy::too_many_arguments)]
#[cfg(test)]
pub fn create_file_v1<S, O, R, C>(
    path: &[u8],
    mode: u16,
    declared_len: u64,
    source: &mut S,
    objects: &mut O,
    references: &mut R,
    buffers: ContentBuffersV1<'_>,
    control: &mut C,
    ledger: &ResourceLedgerV1,
    counters: &mut OperationCountersV1,
) -> CoreResult<PreparedFileV1>
where
    S: ContentSourceV1 + ?Sized,
    O: PreparedObjectSinkV1 + ?Sized,
    R: ChunkReferenceSpoolV1 + ?Sized,
    C: CdcControlV1 + ?Sized,
{
    prepare_file_v1(
        path,
        mode,
        declared_len,
        source,
        objects,
        references,
        buffers,
        control,
        ledger,
        counters,
    )
}

#[allow(clippy::too_many_arguments)]
#[cfg(test)]
pub(super) fn prepare_file_v1<S, O, R, C>(
    path: &[u8],
    mode: u16,
    declared_len: u64,
    source: &mut S,
    objects: &mut O,
    references: &mut R,
    buffers: ContentBuffersV1<'_>,
    control: &mut C,
    ledger: &ResourceLedgerV1,
    counters: &mut OperationCountersV1,
) -> CoreResult<PreparedFileV1>
where
    S: ContentSourceV1 + ?Sized,
    O: PreparedObjectSinkV1 + ?Sized,
    R: ChunkReferenceSpoolV1 + ?Sized,
    C: CdcControlV1 + ?Sized,
{
    ValidatedPath::new(path)?;
    validate_file_mode(mode)?;
    validate_logical_length(declared_len)?;
    let maximum_refs = declared_len
        .checked_add(8_191)
        .ok_or(CoreError::IntegerOverflow)?
        / 8_192;
    validate_chunk_refs_per_file(maximum_refs)?;
    let reference_bytes = references.resident_memory_bound_bytes(maximum_refs)?;
    let source_bytes = source.resident_memory_bound_bytes()?;
    let object_sink_bytes = objects.resident_memory_bound_bytes()?;
    let metadata_bytes = reference_bytes
        .checked_add(source_bytes)
        .and_then(|bytes| bytes.checked_add(object_sink_bytes))
        .ok_or(CoreError::IntegerOverflow)?;

    let memory = OperationMemoryPlanV1::empty()
        .charge(MemoryComponentV1::SourceWindow, buffers.source.len() as u64)?
        .charge(MemoryComponentV1::CdcRing, buffers.cdc_ring.len() as u64)?
        .charge(
            MemoryComponentV1::HashState,
            IDENTITY_HASHER_BYTES_V1
                .checked_mul(2)
                .ok_or(CoreError::IntegerOverflow)?,
        )?
        .charge(MemoryComponentV1::MetadataWindow, metadata_bytes)?;
    let _reservation = ledger.reserve_operation_with_plan(memory)?;
    counters.memory_high_water = counters.memory_high_water.max(ledger.high_water_bytes());
    prepare_file_after_admission(
        mode,
        declared_len,
        maximum_refs,
        source,
        objects,
        references,
        buffers,
        control,
        ContentCdcV1::FastCdc,
        counters,
    )
}

#[cfg(feature = "operation-polymorphism")]
#[allow(clippy::too_many_arguments)]
pub(crate) fn create_file_borrowed_v1<S, O, R, C>(
    path: &[u8],
    mode: u16,
    declared_len: u64,
    source: &mut S,
    objects: &mut O,
    references: &mut R,
    buffers: ContentBuffersV1<'_>,
    control: &mut C,
    reservation: &OperationReservationV1<'_>,
    algorithm: crate::cdc::CdcAlgorithmV1,
    counters: &mut OperationCountersV1,
) -> CoreResult<PreparedFileV1>
where
    S: ContentSourceV1 + ?Sized,
    O: PreparedObjectSinkV1 + ?Sized,
    R: ChunkReferenceSpoolV1 + ?Sized,
    C: CdcControlV1 + ?Sized,
{
    ValidatedPath::new(path)?;
    validate_file_mode(mode)?;
    validate_logical_length(declared_len)?;
    let maximum_refs = declared_len
        .checked_add(8_191)
        .ok_or(CoreError::IntegerOverflow)?
        / 8_192;
    validate_chunk_refs_per_file(maximum_refs)?;
    let reference_bytes = references.resident_memory_bound_bytes(maximum_refs)?;
    let source_bytes = source.resident_memory_bound_bytes()?;
    let object_sink_bytes = objects.resident_memory_bound_bytes()?;
    let metadata_bytes = reference_bytes
        .checked_add(source_bytes)
        .and_then(|bytes| bytes.checked_add(object_sink_bytes))
        .ok_or(CoreError::IntegerOverflow)?;
    let memory = OperationMemoryPlanV1::empty()
        .charge(MemoryComponentV1::SourceWindow, buffers.source.len() as u64)?
        .charge(MemoryComponentV1::CdcRing, buffers.cdc_ring.len() as u64)?
        .charge(
            MemoryComponentV1::HashState,
            IDENTITY_HASHER_BYTES_V1
                .checked_mul(2)
                .ok_or(CoreError::IntegerOverflow)?,
        )?
        .charge(MemoryComponentV1::MetadataWindow, metadata_bytes)?;
    reservation.require(memory)?;
    prepare_file_after_admission(
        mode,
        declared_len,
        maximum_refs,
        source,
        objects,
        references,
        buffers,
        control,
        ContentCdcV1::Selected(algorithm),
        counters,
    )
}

#[derive(Clone, Copy)]
enum ContentCdcV1 {
    FastCdc,
    #[cfg(feature = "operation-polymorphism")]
    Selected(crate::cdc::CdcAlgorithmV1),
}

enum ContentCdcStreamV1<'ring> {
    FastCdc(crate::cdc::FastCdcV1Stream<'ring>),
    #[cfg(feature = "operation-polymorphism")]
    Selected(crate::cdc::CdcStreamV1<'ring>),
}

impl ContentCdcV1 {
    fn stream<'ring, C: CdcControlV1 + ?Sized>(
        self,
        ring: &'ring mut [u8],
        control: &mut C,
    ) -> CoreResult<ContentCdcStreamV1<'ring>> {
        match self {
            Self::FastCdc => FastCdcV1::new()
                .stream(ring, control)
                .map(ContentCdcStreamV1::FastCdc),
            #[cfg(feature = "operation-polymorphism")]
            Self::Selected(algorithm) => algorithm
                .stream(ring, control)
                .map(ContentCdcStreamV1::Selected),
        }
    }
}

impl ContentCdcStreamV1<'_> {
    fn push<C: CdcControlV1 + ?Sized, B: BoundaryConsumerV1 + ?Sized>(
        &mut self,
        fragment: Result<&[u8], CdcSourceErrorV1>,
        control: &mut C,
        consumer: &mut B,
    ) -> CoreResult<()> {
        match self {
            Self::FastCdc(stream) => stream.push(fragment, control, consumer),
            #[cfg(feature = "operation-polymorphism")]
            Self::Selected(stream) => stream.push(fragment, control, consumer),
        }
    }

    fn finish<C: CdcControlV1 + ?Sized, B: BoundaryConsumerV1 + ?Sized>(
        &mut self,
        control: &mut C,
        consumer: &mut B,
    ) -> CoreResult<()> {
        match self {
            Self::FastCdc(stream) => stream.finish(control, consumer),
            #[cfg(feature = "operation-polymorphism")]
            Self::Selected(stream) => stream.finish(control, consumer),
        }
    }

    fn counters(&self) -> crate::cdc::CdcStreamCountersV1 {
        match self {
            Self::FastCdc(stream) => stream.counters(),
            #[cfg(feature = "operation-polymorphism")]
            Self::Selected(stream) => stream.counters(),
        }
    }

    #[cfg(feature = "operation-polymorphism")]
    fn seqcdc_counters(&self) -> Option<crate::cdc::SeqCdcCountersV1> {
        match self {
            Self::FastCdc(_) => None,
            Self::Selected(stream) => stream.seqcdc_counters(),
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn prepare_file_after_admission<S, O, R, C>(
    mode: u16,
    declared_len: u64,
    maximum_refs: u64,
    source: &mut S,
    objects: &mut O,
    references: &mut R,
    buffers: ContentBuffersV1<'_>,
    control: &mut C,
    candidate: ContentCdcV1,
    counters: &mut OperationCountersV1,
) -> CoreResult<PreparedFileV1>
where
    S: ContentSourceV1 + ?Sized,
    O: PreparedObjectSinkV1 + ?Sized,
    R: ChunkReferenceSpoolV1 + ?Sized,
    C: CdcControlV1 + ?Sized,
{
    objects.begin_closure().map_err(map_sink)?;
    if let Err(error) = references.begin(maximum_refs).map_err(map_sink) {
        objects.abort_closure();
        return Err(error);
    }

    let result = (|| {
        let chunk_count = {
            let mut consumer = ChunkConsumerV1 {
                objects,
                references,
                counters,
                chunk_count: 0,
                failure: None,
            };
            let mut stream = candidate.stream(buffers.cdc_ring, control)?;
            let scan_result = (|| {
                let mut remaining = declared_len;
                while remaining != 0 {
                    let request = usize::try_from(remaining.min(MAXIMUM_CHUNK_BYTES as u64))
                        .map_err(|_| CoreError::IntegerOverflow)?;
                    consumer.counters.add(CounterFieldV1::SourceReadCalls, 1)?;
                    let read = source
                        .read(&mut buffers.source[..request])
                        .map_err(|ContentSourceErrorV1::Failure| CoreError::SourceFailure)?;
                    if read > request {
                        return Err(CoreError::SourceFailure);
                    }
                    if read == 0 {
                        return Err(CoreError::Truncated);
                    }
                    let read_u64 = u64::try_from(read).map_err(|_| CoreError::IntegerOverflow)?;
                    consumer.counters.record_source_bytes_read(read_u64)?;
                    consumer
                        .counters
                        .add(CounterFieldV1::BytesCopied, read_u64)?;
                    remaining = remaining
                        .checked_sub(read_u64)
                        .ok_or(CoreError::TrailingBytes)?;
                    if let Err(error) =
                        stream.push(Ok(&buffers.source[..read]), control, &mut consumer)
                    {
                        return Err(consumer.failure.take().unwrap_or(error));
                    }
                }
                consumer.counters.add(CounterFieldV1::SourceReadCalls, 1)?;
                let probe = source
                    .read(&mut buffers.source[..1])
                    .map_err(|ContentSourceErrorV1::Failure| CoreError::SourceFailure)?;
                if probe != 0 {
                    consumer.counters.record_source_bytes_read(1)?;
                    return Err(CoreError::TrailingBytes);
                }
                if let Err(error) = stream.finish(control, &mut consumer) {
                    return Err(consumer.failure.take().unwrap_or(error));
                }
                Ok(())
            })();
            consumer.counters.add_cdc_stream(stream.counters())?;
            #[cfg(feature = "operation-polymorphism")]
            if let Some(seqcdc) = stream.seqcdc_counters() {
                consumer.counters.add_seqcdc(seqcdc)?;
            }
            scan_result?;
            consumer.chunk_count
        };

        let (logical_file, physical_file) = write_file_object_and_logical(
            objects,
            references,
            mode,
            declared_len,
            chunk_count,
            counters,
        )?;
        let chunk_count = u32::try_from(chunk_count).map_err(|_| CoreError::IntegerOverflow)?;
        let prepared = PreparedFileV1 {
            logical_file,
            physical_file,
            chunk_count,
        };
        objects.finish_closure(prepared).map_err(map_sink)?;
        Ok(prepared)
    })();

    if result.is_err() {
        references.abort();
        objects.abort_closure();
    }
    result
}

struct ChunkConsumerV1<'a, O: ?Sized, R: ?Sized> {
    objects: &'a mut O,
    references: &'a mut R,
    counters: &'a mut OperationCountersV1,
    chunk_count: u64,
    failure: Option<CoreError>,
}

impl<O, R> BoundaryConsumerV1 for ChunkConsumerV1<'_, O, R>
where
    O: PreparedObjectSinkV1 + ?Sized,
    R: ChunkReferenceSpoolV1 + ?Sized,
{
    fn accept(
        &mut self,
        boundary: ChunkBoundaryV1,
        chunk: BorrowedChunkV1<'_>,
    ) -> Result<(), CdcBoundaryConsumerErrorV1> {
        let result = self.accept_inner(boundary, chunk);
        if let Err(error) = result {
            self.failure = Some(error);
            Err(CdcBoundaryConsumerErrorV1::Refused)
        } else {
            Ok(())
        }
    }
}

impl<O, R> ChunkConsumerV1<'_, O, R>
where
    O: PreparedObjectSinkV1 + ?Sized,
    R: ChunkReferenceSpoolV1 + ?Sized,
{
    fn accept_inner(
        &mut self,
        boundary: ChunkBoundaryV1,
        chunk: BorrowedChunkV1<'_>,
    ) -> CoreResult<()> {
        if boundary.len() != u64::try_from(chunk.len()).map_err(|_| CoreError::IntegerOverflow)? {
            return Err(CoreError::LogicalLength);
        }
        let chunk_len = u64::try_from(chunk.len()).map_err(|_| CoreError::IntegerOverflow)?;
        self.counters
            .add(CounterFieldV1::LogicalHashBytes, chunk_len)?;
        self.counters.add(
            CounterFieldV1::LogicalHashUpdateCalls,
            u64::from(!chunk.first().is_empty()) + u64::from(!chunk.second().is_empty()),
        )?;
        let logical = derive_logical_chunk_spans_v1(chunk.first(), chunk.second())?;
        let physical = write_chunk_object(self.objects, chunk, self.counters)?;
        let len = u32::try_from(chunk.len()).map_err(|_| CoreError::IntegerOverflow)?;
        self.references
            .push(PreparedChunkRefV1 {
                logical_id: logical.id(),
                physical_id: physical.0,
                len,
            })
            .map_err(map_sink)?;
        self.chunk_count = self
            .chunk_count
            .checked_add(1)
            .ok_or(CoreError::IntegerOverflow)?;
        validate_chunk_refs_per_file(self.chunk_count)?;
        match physical.1 {
            ObjectDispositionV1::Created => {
                self.counters.add(CounterFieldV1::LogicalChunksCreated, 1)?
            }
            ObjectDispositionV1::Reused => {
                self.counters.add(CounterFieldV1::LogicalChunksReused, 1)?
            }
        }
        Ok(())
    }
}

pub(crate) fn write_chunk_object<O: PreparedObjectSinkV1 + ?Sized>(
    objects: &mut O,
    chunk: BorrowedChunkV1<'_>,
    counters: &mut OperationCountersV1,
) -> CoreResult<(PhysicalChunkIdV1, ObjectDispositionV1)> {
    let payload_len = u64::try_from(chunk.len()).map_err(|_| CoreError::IntegerOverflow)?;
    let complete_len = OBJECT_HEADER_BYTES
        .checked_add(payload_len)
        .ok_or(CoreError::IntegerOverflow)?;
    objects
        .begin_object(PhysicalObjectKindV1::Chunk, complete_len)
        .map_err(map_sink)?;
    let mut hasher = FramedHasherV1::new(TAG_PHYSICAL_CHUNK, complete_len);
    let header = encode_physical_object_header_v1(PhysicalObjectKindV1::Chunk, payload_len);
    write_segment(objects, &mut hasher, &header, counters)?;
    write_segment(objects, &mut hasher, chunk.first(), counters)?;
    write_segment(objects, &mut hasher, chunk.second(), counters)?;
    let id = PhysicalChunkIdV1::from_digest(hasher.finish()?);
    let disposition = objects
        .finish_object(TypedPhysicalObjectIdV1::Chunk(id))
        .map_err(map_sink)?;
    count_disposition(counters, disposition)?;
    Ok((id, disposition))
}

pub(crate) fn write_file_object_and_logical<
    O: PreparedObjectSinkV1 + ?Sized,
    R: ChunkReferenceSpoolV1 + ?Sized,
>(
    objects: &mut O,
    references: &mut R,
    mode: u16,
    logical_len: u64,
    chunk_count: u64,
    counters: &mut OperationCountersV1,
) -> CoreResult<(LogicalFileIdentityV1, PhysicalFileIdV1)> {
    let extent_count = u32::from(chunk_count != 0);
    let references_len = chunk_count
        .checked_mul(CHUNK_REFERENCE_BYTES)
        .ok_or(CoreError::IntegerOverflow)?;
    let payload_len = FILE_FIXED_PAYLOAD_BYTES
        .checked_add(if chunk_count == 0 {
            0
        } else {
            DATA_EXTENT_FIXED_BYTES
                .checked_add(references_len)
                .ok_or(CoreError::IntegerOverflow)?
        })
        .ok_or(CoreError::IntegerOverflow)?;
    let complete_len = OBJECT_HEADER_BYTES
        .checked_add(payload_len)
        .ok_or(CoreError::IntegerOverflow)?;
    objects
        .begin_object(PhysicalObjectKindV1::File, complete_len)
        .map_err(map_sink)?;
    let mut physical_hasher = FramedHasherV1::new(TAG_PHYSICAL_FILE, complete_len);
    let mut logical_hasher = LogicalFileHasherV1::new(logical_len, chunk_count)?;
    let header = encode_physical_object_header_v1(PhysicalObjectKindV1::File, payload_len);
    write_segment(objects, &mut physical_hasher, &header, counters)?;
    write_segment(objects, &mut physical_hasher, &mode.to_be_bytes(), counters)?;
    write_segment(
        objects,
        &mut physical_hasher,
        &logical_len.to_be_bytes(),
        counters,
    )?;
    write_segment(
        objects,
        &mut physical_hasher,
        &extent_count.to_be_bytes(),
        counters,
    )?;
    if chunk_count != 0 {
        write_segment(objects, &mut physical_hasher, &[0x02], counters)?;
        write_segment(
            objects,
            &mut physical_hasher,
            &logical_len.to_be_bytes(),
            counters,
        )?;
        let count = u32::try_from(chunk_count).map_err(|_| CoreError::IntegerOverflow)?;
        write_segment(
            objects,
            &mut physical_hasher,
            &count.to_be_bytes(),
            counters,
        )?;
        references.rewind().map_err(map_sink)?;
        for _ in 0..chunk_count {
            let chunk = references
                .next()
                .map_err(map_sink)?
                .ok_or(CoreError::Truncated)?;
            logical_hasher.push(chunk.logical_ref())?;
            write_segment(
                objects,
                &mut physical_hasher,
                &chunk.len.to_be_bytes(),
                counters,
            )?;
            write_segment(
                objects,
                &mut physical_hasher,
                chunk.physical_id.as_bytes(),
                counters,
            )?;
        }
        if references.next().map_err(map_sink)?.is_some() {
            return Err(CoreError::TrailingBytes);
        }
    }
    let logical = logical_hasher.finish()?;
    let id = PhysicalFileIdV1::from_digest(physical_hasher.finish()?);
    let disposition = objects
        .finish_object(TypedPhysicalObjectIdV1::File(id))
        .map_err(map_sink)?;
    count_disposition(counters, disposition)?;
    Ok((logical, id))
}

pub(crate) fn file_object_lengths_v1(chunk_count: u64) -> CoreResult<(u64, u64)> {
    let references_len = chunk_count
        .checked_mul(CHUNK_REFERENCE_BYTES)
        .ok_or(CoreError::IntegerOverflow)?;
    let payload_len = FILE_FIXED_PAYLOAD_BYTES
        .checked_add(if chunk_count == 0 {
            0
        } else {
            DATA_EXTENT_FIXED_BYTES
                .checked_add(references_len)
                .ok_or(CoreError::IntegerOverflow)?
        })
        .ok_or(CoreError::IntegerOverflow)?;
    let complete_len = OBJECT_HEADER_BYTES
        .checked_add(payload_len)
        .ok_or(CoreError::IntegerOverflow)?;
    Ok((payload_len, complete_len))
}

fn write_segment<O: PreparedObjectSinkV1 + ?Sized>(
    objects: &mut O,
    hasher: &mut FramedHasherV1,
    bytes: &[u8],
    counters: &mut OperationCountersV1,
) -> CoreResult<()> {
    hasher.write(bytes)?;
    let len = u64::try_from(bytes.len()).map_err(|_| CoreError::IntegerOverflow)?;
    counters.add(CounterFieldV1::PhysicalHashBytes, len)?;
    counters.add(CounterFieldV1::PhysicalHashUpdateCalls, 1)?;
    objects.write_private(bytes).map_err(map_sink)?;
    counters.add(CounterFieldV1::BytesWritten, len)
}

fn count_disposition(
    counters: &mut OperationCountersV1,
    disposition: ObjectDispositionV1,
) -> CoreResult<()> {
    let field = match disposition {
        ObjectDispositionV1::Created => CounterFieldV1::PhysicalObjectsCreated,
        ObjectDispositionV1::Reused => CounterFieldV1::PhysicalObjectsReused,
    };
    counters.add(field, 1)
}

fn map_sink(PreparedSinkErrorV1::Refused: PreparedSinkErrorV1) -> CoreError {
    CoreError::SinkRefused
}

impl From<ContentSourceErrorV1> for CdcSourceErrorV1 {
    fn from(ContentSourceErrorV1::Failure: ContentSourceErrorV1) -> Self {
        Self::Failure
    }
}
