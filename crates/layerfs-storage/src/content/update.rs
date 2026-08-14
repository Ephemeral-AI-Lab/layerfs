//! Exact bounded range-resynchronizing Update.
//!
//! The byte reader is positional and is invoked only for the predecessor
//! prefix and at most 65,536 old suffix bytes. Whole-file chunk-reference
//! metadata is replayed to authenticate the base logical identity and to
//! structurally reuse untouched references; base payload is never scanned.

pub(crate) use crate::cdc::MAX_UPDATE_RESYNCHRONIZATION_BYTES;
use crate::cdc::{
    BorrowedChunkV1, BoundaryConsumerV1, CdcBoundaryConsumerErrorV1, CdcControlV1, ChunkBoundaryV1,
    FastCdcV1, RejoinOperationBindingV1, VerifiedRejoinV1, MAXIMUM_CHUNK_BYTES,
};
use crate::content::{
    write_chunk_object, write_file_object_and_logical, ChunkReferenceSpoolV1, ContentSourceErrorV1,
    ContentSourceV1, ObjectDispositionV1, PreparedChunkRefV1, PreparedFileV1, PreparedObjectSinkV1,
    PreparedSinkErrorV1,
};
pub(crate) use crate::cow::file::{AuthenticatedBaseFileV1, UpdateRangeV1};
use crate::format::{
    validate_chunk_reference_len, validate_chunk_refs_per_file, validate_file_mode,
    validate_logical_length, ValidatedPath,
};
use crate::identity::{
    derive_logical_chunk_spans_v1, LogicalChunkIdV1, LogicalFileHasherV1, PhysicalChunkIdV1,
    IDENTITY_HASHER_BYTES_V1,
};
use crate::limits::OperationReservationV1;
use crate::limits::ResourceLedgerV1;
use crate::limits::{
    CounterFieldV1, MemoryComponentV1, OperationCountersV1, OperationMemoryPlanV1,
};
use crate::object::CanonicalFileObjectEncoderV1;
use crate::{CoreError, CoreResult};

const CHUNK_REFERENCE_METADATA_BYTES: u64 = 36;

pub(crate) struct UpdateBuffersV1<'buffers> {
    source: &'buffers mut [u8; MAXIMUM_CHUNK_BYTES],
    cdc_ring: &'buffers mut [u8; MAXIMUM_CHUNK_BYTES],
}

impl<'buffers> UpdateBuffersV1<'buffers> {
    pub(crate) fn new(
        source: &'buffers mut [u8; MAXIMUM_CHUNK_BYTES],
        cdc_ring: &'buffers mut [u8; MAXIMUM_CHUNK_BYTES],
    ) -> Self {
        Self { source, cdc_ring }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BaseChunkEvidenceV1 {
    start: u64,
    logical_id: LogicalChunkIdV1,
    physical_id: PhysicalChunkIdV1,
    len: u32,
}

impl BaseChunkEvidenceV1 {
    pub const fn new(
        start: u64,
        logical_id: LogicalChunkIdV1,
        physical_id: PhysicalChunkIdV1,
        len: u32,
    ) -> Self {
        Self {
            start,
            logical_id,
            physical_id,
            len,
        }
    }

    pub const fn start(self) -> u64 {
        self.start
    }

    pub fn end(self) -> CoreResult<u64> {
        self.start
            .checked_add(u64::from(self.len))
            .ok_or(CoreError::RangeResyncFailed)
    }

    pub const fn logical_id(self) -> LogicalChunkIdV1 {
        self.logical_id
    }

    pub const fn physical_id(self) -> PhysicalChunkIdV1 {
        self.physical_id
    }

    pub const fn len(self) -> u32 {
        self.len
    }

    pub const fn is_empty(self) -> bool {
        self.len == 0
    }

    fn prepared(self) -> PreparedChunkRefV1 {
        PreparedChunkRefV1::from_parts(self.logical_id, self.physical_id, self.len)
    }
}

/// Authenticated, replayable boundary evidence. Positional lookup is metadata
/// lookup only and must not read base payload bytes. The retained evidence
/// carrier is immutable-storage state, but every userspace window or cache
/// needed by this operation must be declared and charged before it is used.
pub trait BaseChunkEvidenceSourceV1 {
    /// Pure declaration queried before admission. It must not allocate,
    /// cache, traverse evidence, or perform I/O.
    fn resident_memory_bound_bytes(&self) -> CoreResult<u64>;
    fn rewind(&mut self) -> Result<(), PreparedSinkErrorV1>;
    fn next(&mut self) -> Result<Option<BaseChunkEvidenceV1>, PreparedSinkErrorV1>;
    fn containing(
        &mut self,
        offset: u64,
        include_end: bool,
    ) -> Result<Option<BaseChunkEvidenceV1>, PreparedSinkErrorV1>;
    fn at_start(&mut self, offset: u64)
        -> Result<Option<BaseChunkEvidenceV1>, PreparedSinkErrorV1>;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BaseReadErrorV1 {
    Missing,
    Failure,
}

pub trait AuthenticatedBaseByteReaderV1 {
    /// Maximum userspace-resident working memory used by this reader for one
    /// Update. The immutable base object itself is storage, not an operation
    /// buffer, but decoder windows, caches, and comparison scratch belong in
    /// this declaration.
    /// This is a pure pre-admission declaration and must perform no read,
    /// allocation, or cache initialization.
    fn resident_memory_bound_bytes(&self) -> CoreResult<u64>;

    fn read_exact_at(&mut self, offset: u64, destination: &mut [u8])
        -> Result<(), BaseReadErrorV1>;

    /// Compares the authenticated base bytes at `offset` with both borrowed
    /// candidate spans, in order. Implementations must compare every byte,
    /// must not allocate or retain either span, and must not fall back to a
    /// whole-file read or cache.
    fn compare_exact_at(
        &mut self,
        offset: u64,
        first: &[u8],
        second: &[u8],
    ) -> Result<bool, BaseReadErrorV1>;
}

#[allow(clippy::too_many_arguments)]
pub fn update_file_v1<S, O, R, E, B, C>(
    path: &[u8],
    mode: u16,
    base: AuthenticatedBaseFileV1,
    range: UpdateRangeV1,
    inserted_len: u64,
    inserted: &mut S,
    base_bytes: &mut B,
    evidence: &mut E,
    objects: &mut O,
    output: &mut R,
    buffers: UpdateBuffersV1<'_>,
    control: &mut C,
    ledger: &ResourceLedgerV1,
    counters: &mut OperationCountersV1,
) -> CoreResult<PreparedFileV1>
where
    S: ContentSourceV1 + ?Sized,
    O: PreparedObjectSinkV1 + ?Sized,
    R: ChunkReferenceSpoolV1 + ?Sized,
    E: BaseChunkEvidenceSourceV1 + ?Sized,
    B: AuthenticatedBaseByteReaderV1 + ?Sized,
    C: CdcControlV1 + ?Sized,
{
    let result = update_file_inner(
        path,
        mode,
        base,
        range,
        inserted_len,
        inserted,
        base_bytes,
        evidence,
        objects,
        output,
        buffers,
        control,
        UpdateMemoryAdmissionV1::Independent(ledger),
        counters,
    );
    if result.is_err() {
        let _ = counters.add(CounterFieldV1::UpdateFailures, 1);
    }
    result
}

/// Bounded semantic Update adapter for the default mutation owner. The
/// request contains bytes, a range, and finite fault/residency controls; the
/// result contains only immutable identities, typed failure, and work/cleanup
/// observations. Internal evidence and ports stay private to this module.
pub mod semantic {
    use super::{
        update_file_v1, AuthenticatedBaseByteReaderV1, AuthenticatedBaseFileV1,
        BaseChunkEvidenceSourceV1, BaseChunkEvidenceV1, BaseReadErrorV1, UpdateRangeV1,
    };
    use crate::cdc::{FastCdcV1, MAXIMUM_CHUNK_BYTES};
    use crate::content::{
        ChunkReferenceSpoolV1, ContentSourceErrorV1, ContentSourceV1, ObjectDispositionV1,
        PreparedChunkRefV1, PreparedFileV1, PreparedObjectSinkV1, PreparedSinkErrorV1,
    };
    use crate::format::PhysicalObjectKindV1;
    use crate::identity::{
        derive_logical_chunk_v1, derive_logical_file_v1, derive_physical_chunk_id_v1,
        derive_physical_file_id_v1, LogicalChunkRefV1, LogicalFileIdentityV1, PhysicalFileIdV1,
        IDENTITY_HASHER_BYTES_V1,
    };
    use crate::limits::{OperationCountersV1, ResourceLedgerV1, BASE_LEDGER_BYTES};
    use crate::profile::ProfileSpecV1;
    use crate::{CoreError, CoreResult};

    const DIGEST_BYTES: usize = crate::identity::DIGEST_BYTES;

    #[derive(Clone, Copy, Debug)]
    pub struct UpdateRequestV1<'a> {
        base: &'a [u8],
        start: u64,
        end: u64,
        inserted: &'a [u8],
        mode: u16,
        budget_bytes: u64,
        base_resident_bytes: u64,
        evidence_resident_bytes: u64,
        base_failure_call: Option<u64>,
        missing_evidence: bool,
        mismatched_logical: bool,
        mismatched_physical: bool,
        no_anchor: bool,
    }

    impl<'a> UpdateRequestV1<'a> {
        pub const fn new(base: &'a [u8], start: u64, end: u64, inserted: &'a [u8]) -> Self {
            Self {
                base,
                start,
                end,
                inserted,
                mode: 0o644,
                budget_bytes: 32 * 1024 * 1024,
                base_resident_bytes: 0,
                evidence_resident_bytes: 0,
                base_failure_call: None,
                missing_evidence: false,
                mismatched_logical: false,
                mismatched_physical: false,
                no_anchor: false,
            }
        }

        pub const fn with_mode(mut self, mode: u16) -> Self {
            self.mode = mode;
            self
        }

        pub const fn with_budget(mut self, budget_bytes: u64) -> Self {
            self.budget_bytes = budget_bytes;
            self
        }

        pub const fn with_base_residency(mut self, bytes: u64) -> Self {
            self.base_resident_bytes = bytes;
            self
        }

        pub const fn with_evidence_residency(mut self, bytes: u64) -> Self {
            self.evidence_resident_bytes = bytes;
            self
        }

        pub const fn with_base_failure_on(mut self, call: u64) -> Self {
            self.base_failure_call = Some(call);
            self
        }

        pub const fn with_missing_evidence(mut self, missing: bool) -> Self {
            self.missing_evidence = missing;
            self
        }

        pub const fn with_mismatched_logical(mut self, mismatched: bool) -> Self {
            self.mismatched_logical = mismatched;
            self
        }

        pub const fn with_mismatched_physical(mut self, mismatched: bool) -> Self {
            self.mismatched_physical = mismatched;
            self
        }

        pub const fn with_mismatched_base(mut self, mismatched: bool) -> Self {
            self.mismatched_logical = mismatched;
            self.mismatched_physical = mismatched;
            self
        }

        pub const fn with_no_anchor(mut self, no_anchor: bool) -> Self {
            self.no_anchor = no_anchor;
            self
        }
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub struct UpdateObservationV1 {
        error: Option<CoreError>,
        logical_id: [u8; DIGEST_BYTES],
        physical_id: [u8; DIGEST_BYTES],
        base_bytes_read: u64,
        base_read_calls: u64,
        inserted_reads: u64,
        output_ref_count: u64,
        prepared_chunk_count: u64,
        sink_object_count: u64,
        sink_completed: bool,
        sink_aborts: u64,
        output_aborted: bool,
        planned_memory_high_water: u64,
        admitted_slots: u64,
        update_resynchronization_bytes: u64,
        logical_chunks_reused: u64,
        bytes_structurally_reused: u64,
        anchor_attempts: u64,
        automatic_fallbacks: u64,
        redispatches: u64,
        publication_authority_dispatches: u64,
        update_failures: u64,
    }

    impl UpdateObservationV1 {
        pub const fn error(self) -> Option<CoreError> {
            self.error
        }

        pub const fn logical_id(self) -> [u8; DIGEST_BYTES] {
            self.logical_id
        }

        pub const fn physical_id(self) -> [u8; DIGEST_BYTES] {
            self.physical_id
        }

        pub const fn base_bytes_read(self) -> u64 {
            self.base_bytes_read
        }

        pub const fn base_read_calls(self) -> u64 {
            self.base_read_calls
        }

        pub const fn inserted_reads(self) -> u64 {
            self.inserted_reads
        }

        pub const fn output_ref_count(self) -> u64 {
            self.output_ref_count
        }

        pub const fn prepared_chunk_count(self) -> u64 {
            self.prepared_chunk_count
        }

        pub const fn sink_object_count(self) -> u64 {
            self.sink_object_count
        }

        pub const fn sink_completed(self) -> bool {
            self.sink_completed
        }

        pub const fn sink_aborts(self) -> u64 {
            self.sink_aborts
        }

        pub const fn output_aborted(self) -> bool {
            self.output_aborted
        }

        pub const fn planned_memory_high_water(self) -> u64 {
            self.planned_memory_high_water
        }

        pub const fn admitted_slots(self) -> u64 {
            self.admitted_slots
        }

        pub const fn update_resynchronization_bytes(self) -> u64 {
            self.update_resynchronization_bytes
        }

        pub const fn logical_chunks_reused(self) -> u64 {
            self.logical_chunks_reused
        }

        pub const fn bytes_structurally_reused(self) -> u64 {
            self.bytes_structurally_reused
        }

        pub const fn anchor_attempts(self) -> u64 {
            self.anchor_attempts
        }

        pub const fn automatic_fallbacks(self) -> u64 {
            self.automatic_fallbacks
        }

        pub const fn redispatches(self) -> u64 {
            self.redispatches
        }

        pub const fn publication_authority_dispatches(self) -> u64 {
            self.publication_authority_dispatches
        }

        pub const fn update_failures(self) -> u64 {
            self.update_failures
        }
    }

    struct Source<'a> {
        reader: &'a mut dyn std::io::Read,
        reads: u64,
    }

    impl<'a> Source<'a> {
        fn new(reader: &'a mut dyn std::io::Read) -> Self {
            Self { reader, reads: 0 }
        }
    }

    impl ContentSourceV1 for Source<'_> {
        fn resident_memory_bound_bytes(&self) -> CoreResult<u64> {
            Ok(0)
        }

        fn read(&mut self, destination: &mut [u8]) -> Result<usize, ContentSourceErrorV1> {
            self.reads += 1;
            self.reader
                .read(destination)
                .map_err(|_| ContentSourceErrorV1::Failure)
        }
    }

    struct BaseBytes<'a> {
        bytes: &'a [u8],
        calls: u64,
        bytes_read: u64,
        fail_on_call: Option<u64>,
        resident_bytes: u64,
    }

    impl AuthenticatedBaseByteReaderV1 for BaseBytes<'_> {
        fn resident_memory_bound_bytes(&self) -> CoreResult<u64> {
            Ok(self.resident_bytes)
        }

        fn read_exact_at(
            &mut self,
            offset: u64,
            destination: &mut [u8],
        ) -> Result<(), BaseReadErrorV1> {
            self.calls += 1;
            if self.fail_on_call == Some(self.calls) {
                return Err(BaseReadErrorV1::Failure);
            }
            let start = usize::try_from(offset).map_err(|_| BaseReadErrorV1::Missing)?;
            let end = start
                .checked_add(destination.len())
                .ok_or(BaseReadErrorV1::Missing)?;
            destination
                .copy_from_slice(self.bytes.get(start..end).ok_or(BaseReadErrorV1::Missing)?);
            self.bytes_read += destination.len() as u64;
            Ok(())
        }

        fn compare_exact_at(
            &mut self,
            offset: u64,
            first: &[u8],
            second: &[u8],
        ) -> Result<bool, BaseReadErrorV1> {
            self.calls += 1;
            if self.fail_on_call == Some(self.calls) {
                return Err(BaseReadErrorV1::Failure);
            }
            let start = usize::try_from(offset).map_err(|_| BaseReadErrorV1::Missing)?;
            let first_end = start
                .checked_add(first.len())
                .ok_or(BaseReadErrorV1::Missing)?;
            let end = first_end
                .checked_add(second.len())
                .ok_or(BaseReadErrorV1::Missing)?;
            let expected_first = self
                .bytes
                .get(start..first_end)
                .ok_or(BaseReadErrorV1::Missing)?;
            let expected_second = self
                .bytes
                .get(first_end..end)
                .ok_or(BaseReadErrorV1::Missing)?;
            self.bytes_read += (first.len() + second.len()) as u64;
            Ok(first == expected_first && second == expected_second)
        }
    }

    struct Evidence {
        chunks: Vec<BaseChunkEvidenceV1>,
        cursor: usize,
        resident_bytes: u64,
        no_anchor: bool,
    }

    impl BaseChunkEvidenceSourceV1 for Evidence {
        fn resident_memory_bound_bytes(&self) -> CoreResult<u64> {
            Ok(self.resident_bytes)
        }

        fn rewind(&mut self) -> Result<(), PreparedSinkErrorV1> {
            self.cursor = 0;
            Ok(())
        }

        fn next(&mut self) -> Result<Option<BaseChunkEvidenceV1>, PreparedSinkErrorV1> {
            let result = self.chunks.get(self.cursor).copied();
            self.cursor += usize::from(result.is_some());
            Ok(result)
        }

        fn containing(
            &mut self,
            offset: u64,
            include_end: bool,
        ) -> Result<Option<BaseChunkEvidenceV1>, PreparedSinkErrorV1> {
            let total = self
                .chunks
                .last()
                .and_then(|chunk| chunk.end().ok())
                .unwrap_or(0);
            Ok(self.chunks.iter().copied().find(|chunk| {
                let end = chunk.end().unwrap();
                (chunk.start() <= offset && offset < end)
                    || (include_end && offset == end && end == total)
            }))
        }

        fn at_start(
            &mut self,
            offset: u64,
        ) -> Result<Option<BaseChunkEvidenceV1>, PreparedSinkErrorV1> {
            if self.no_anchor {
                Ok(None)
            } else {
                Ok(self
                    .chunks
                    .iter()
                    .copied()
                    .find(|chunk| chunk.start() == offset))
            }
        }
    }

    #[derive(Default)]
    struct Spool {
        refs: Vec<PreparedChunkRefV1>,
        cursor: usize,
        maximum: u64,
        aborted: bool,
    }

    impl ChunkReferenceSpoolV1 for Spool {
        fn resident_memory_bound_bytes(&self, maximum_refs: u64) -> CoreResult<u64> {
            maximum_refs
                .checked_mul(core::mem::size_of::<PreparedChunkRefV1>() as u64)
                .ok_or(CoreError::IntegerOverflow)
        }

        fn begin(&mut self, maximum_refs: u64) -> Result<(), PreparedSinkErrorV1> {
            self.refs.clear();
            self.cursor = 0;
            self.maximum = maximum_refs;
            self.aborted = false;
            Ok(())
        }

        fn push(&mut self, chunk: PreparedChunkRefV1) -> Result<(), PreparedSinkErrorV1> {
            if self.refs.len() as u64 >= self.maximum {
                return Err(PreparedSinkErrorV1::Refused);
            }
            self.refs.push(chunk);
            Ok(())
        }

        fn rewind(&mut self) -> Result<(), PreparedSinkErrorV1> {
            self.cursor = 0;
            Ok(())
        }

        fn next(&mut self) -> Result<Option<PreparedChunkRefV1>, PreparedSinkErrorV1> {
            let result = self.refs.get(self.cursor).copied();
            self.cursor += usize::from(result.is_some());
            Ok(result)
        }

        fn abort(&mut self) {
            self.refs.clear();
            self.aborted = true;
        }
    }

    #[derive(Default)]
    struct Sink {
        declared: u64,
        written: u64,
        object_ids: Vec<crate::object::TypedPhysicalObjectIdV1>,
        completed: bool,
        aborts: u64,
    }

    impl PreparedObjectSinkV1 for Sink {
        fn resident_memory_bound_bytes(&self) -> CoreResult<u64> {
            Ok(0)
        }

        fn begin_closure(&mut self) -> Result<(), PreparedSinkErrorV1> {
            Ok(())
        }

        fn begin_object(
            &mut self,
            _kind: PhysicalObjectKindV1,
            complete_len: u64,
        ) -> Result<(), PreparedSinkErrorV1> {
            self.declared = complete_len;
            self.written = 0;
            Ok(())
        }

        fn write_private(&mut self, bytes: &[u8]) -> Result<(), PreparedSinkErrorV1> {
            self.written += bytes.len() as u64;
            Ok(())
        }

        fn finish_object(
            &mut self,
            expected_id: crate::object::TypedPhysicalObjectIdV1,
        ) -> Result<ObjectDispositionV1, PreparedSinkErrorV1> {
            if self.written != self.declared {
                return Err(PreparedSinkErrorV1::Refused);
            }
            self.object_ids.push(expected_id);
            Ok(ObjectDispositionV1::Created)
        }

        fn finish_closure(&mut self, _result: PreparedFileV1) -> Result<(), PreparedSinkErrorV1> {
            self.completed = true;
            Ok(())
        }

        fn abort_closure(&mut self) {
            self.aborts += 1;
            self.completed = false;
            self.object_ids.clear();
        }
    }

    fn object(kind: u8, payload: &[u8]) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(52 + payload.len());
        bytes.extend_from_slice(b"ELSOBJ01");
        bytes.extend_from_slice(&1_u16.to_be_bytes());
        bytes.push(kind);
        bytes.push(0);
        bytes.extend_from_slice(ProfileSpecV1::frozen().id().as_bytes());
        bytes.extend_from_slice(&(payload.len() as u64).to_be_bytes());
        bytes.extend_from_slice(payload);
        bytes
    }

    fn fixture(
        data: &[u8],
        mode: u16,
    ) -> (
        LogicalFileIdentityV1,
        PhysicalFileIdV1,
        Vec<BaseChunkEvidenceV1>,
    ) {
        let mut chunks = Vec::new();
        let mut logical_refs = Vec::new();
        let mut physical_refs = Vec::new();
        let mut offset = 0_usize;
        while offset < data.len() {
            let cut = FastCdcV1::new().cut(&data[offset..]).unwrap();
            let payload = &data[offset..offset + cut];
            let logical = derive_logical_chunk_v1(payload).unwrap();
            let physical = derive_physical_chunk_id_v1(&object(0x05, payload)).unwrap();
            chunks.push(BaseChunkEvidenceV1::new(
                offset as u64,
                logical.id(),
                physical,
                cut as u32,
            ));
            logical_refs.push(LogicalChunkRefV1::from_identity(logical));
            physical_refs.push((cut as u32, physical));
            offset += cut;
        }
        let logical = derive_logical_file_v1(data.len() as u64, &logical_refs).unwrap();
        let mut payload = Vec::new();
        payload.extend_from_slice(&mode.to_be_bytes());
        payload.extend_from_slice(&(data.len() as u64).to_be_bytes());
        payload.extend_from_slice(&u32::from(!data.is_empty()).to_be_bytes());
        if !data.is_empty() {
            payload.push(0x02);
            payload.extend_from_slice(&(data.len() as u64).to_be_bytes());
            payload.extend_from_slice(&(physical_refs.len() as u32).to_be_bytes());
            for (len, id) in physical_refs {
                payload.extend_from_slice(&len.to_be_bytes());
                payload.extend_from_slice(id.as_bytes());
            }
        }
        let physical = derive_physical_file_id_v1(&object(0x03, &payload)).unwrap();
        (logical, physical, chunks)
    }

    fn empty_observation(error: CoreError) -> UpdateObservationV1 {
        UpdateObservationV1 {
            error: Some(error),
            logical_id: [0; DIGEST_BYTES],
            physical_id: [0; DIGEST_BYTES],
            base_bytes_read: 0,
            base_read_calls: 0,
            inserted_reads: 0,
            output_ref_count: 0,
            prepared_chunk_count: 0,
            sink_object_count: 0,
            sink_completed: false,
            sink_aborts: 0,
            output_aborted: false,
            planned_memory_high_water: BASE_LEDGER_BYTES,
            admitted_slots: 0,
            update_resynchronization_bytes: 0,
            logical_chunks_reused: 0,
            bytes_structurally_reused: 0,
            anchor_attempts: 0,
            automatic_fallbacks: 0,
            redispatches: 0,
            publication_authority_dispatches: 0,
            update_failures: 0,
        }
    }

    pub fn update_v1(request: &UpdateRequestV1<'_>) -> UpdateObservationV1 {
        let mut reader = std::io::Cursor::new(request.inserted);
        update_from_reader_v1(request, request.inserted.len() as u64, &mut reader)
    }

    pub fn update_from_reader_v1(
        request: &UpdateRequestV1<'_>,
        inserted_len: u64,
        reader: &mut dyn std::io::Read,
    ) -> UpdateObservationV1 {
        let (logical, physical, chunks) = fixture(request.base, request.mode);
        let (authenticated, evidence_chunks) = if request.mismatched_logical
            || request.mismatched_physical
        {
            let (other_logical, other_physical, _) = fixture(b"different", request.mode);
            (
                AuthenticatedBaseFileV1::new(
                    if request.mismatched_logical {
                        other_logical
                    } else {
                        logical
                    },
                    if request.mismatched_physical {
                        other_physical
                    } else {
                        physical
                    },
                    request.mode,
                    chunks.len() as u32,
                ),
                chunks,
            )
        } else {
            (
                AuthenticatedBaseFileV1::new(logical, physical, request.mode, chunks.len() as u32),
                chunks,
            )
        };
        let range = match UpdateRangeV1::new(request.start, request.end, request.base.len() as u64)
        {
            Ok(range) => range,
            Err(error) => return empty_observation(error),
        };
        let mut evidence = Evidence {
            chunks: evidence_chunks,
            cursor: 0,
            resident_bytes: request.evidence_resident_bytes,
            no_anchor: request.no_anchor,
        };
        if request.missing_evidence {
            evidence.chunks.pop();
        }
        let mut source = Source::new(reader);
        let mut base_bytes = BaseBytes {
            bytes: request.base,
            calls: 0,
            bytes_read: 0,
            fail_on_call: request.base_failure_call,
            resident_bytes: request.base_resident_bytes,
        };
        let mut sink = Sink::default();
        let mut output = Spool::default();
        let mut source_buffer = [0_u8; MAXIMUM_CHUNK_BYTES];
        let mut ring = [0_u8; MAXIMUM_CHUNK_BYTES];
        let ledger = ResourceLedgerV1::new(request.budget_bytes);
        let mut counters = OperationCountersV1::default();
        let prepared = update_file_v1(
            b"file.bin",
            request.mode,
            authenticated,
            range,
            inserted_len,
            &mut source,
            &mut base_bytes,
            &mut evidence,
            &mut sink,
            &mut output,
            super::UpdateBuffersV1::new(&mut source_buffer, &mut ring),
            &mut crate::cdc::ContinueCdcControlV1,
            &ledger,
            &mut counters,
        );
        let (logical_id, physical_id) = prepared
            .as_ref()
            .map(|prepared| {
                (
                    *prepared.logical_file().id().as_bytes(),
                    *prepared.physical_file().as_bytes(),
                )
            })
            .unwrap_or(([0; DIGEST_BYTES], [0; DIGEST_BYTES]));
        let prepared_chunk_count = prepared
            .as_ref()
            .map(|prepared| prepared.chunk_count() as u64)
            .unwrap_or(0);
        UpdateObservationV1 {
            error: prepared.as_ref().err().copied(),
            logical_id,
            physical_id,
            base_bytes_read: base_bytes.bytes_read,
            base_read_calls: base_bytes.calls,
            inserted_reads: source.reads,
            output_ref_count: output.refs.len() as u64,
            prepared_chunk_count,
            sink_object_count: sink.object_ids.len() as u64,
            sink_completed: sink.completed,
            sink_aborts: sink.aborts,
            output_aborted: output.aborted,
            planned_memory_high_water: ledger.planned_high_water_bytes(),
            admitted_slots: ledger.admitted_slots(),
            update_resynchronization_bytes: counters.update_resynchronization_bytes,
            logical_chunks_reused: counters.logical_chunks_reused,
            bytes_structurally_reused: counters.bytes_structurally_reused,
            anchor_attempts: counters.anchor_attempts,
            automatic_fallbacks: counters.automatic_fallbacks,
            redispatches: counters.redispatches,
            publication_authority_dispatches: counters.publication_authority_dispatches,
            update_failures: counters.update_failures,
        }
    }

    pub const fn identity_hasher_bytes() -> u64 {
        IDENTITY_HASHER_BYTES_V1
    }

    pub const fn expected_planned_high_water(bytes: u64) -> u64 {
        BASE_LEDGER_BYTES
            + (MAXIMUM_CHUNK_BYTES as u64 * 2)
            + (IDENTITY_HASHER_BYTES_V1 * 2)
            + bytes.div_ceil(8_192) * core::mem::size_of::<PreparedChunkRefV1>() as u64
    }

    pub const fn max_update_resynchronization_bytes() -> u64 {
        super::MAX_UPDATE_RESYNCHRONIZATION_BYTES
    }

    pub fn first_chunk_end(bytes: &[u8]) -> CoreResult<u64> {
        FastCdcV1::new().cut(bytes).map(|length| length as u64)
    }
}

/// Complete-operation adapter which borrows the already granted root operation.
/// It cannot mint another ledger slot and therefore preserves the single
/// operation capability across verified rejoin and immutable staging.
#[cfg(feature = "operation-polymorphism")]
#[allow(clippy::too_many_arguments)]
pub(crate) fn update_file_borrowed_v1<S, O, R, E, B, C>(
    path: &[u8],
    mode: u16,
    base: AuthenticatedBaseFileV1,
    range: UpdateRangeV1,
    inserted_len: u64,
    inserted: &mut S,
    base_bytes: &mut B,
    evidence: &mut E,
    objects: &mut O,
    output: &mut R,
    buffers: UpdateBuffersV1<'_>,
    control: &mut C,
    reservation: &OperationReservationV1<'_>,
    counters: &mut OperationCountersV1,
) -> CoreResult<PreparedFileV1>
where
    S: ContentSourceV1 + ?Sized,
    O: PreparedObjectSinkV1 + ?Sized,
    R: ChunkReferenceSpoolV1 + ?Sized,
    E: BaseChunkEvidenceSourceV1 + ?Sized,
    B: AuthenticatedBaseByteReaderV1 + ?Sized,
    C: CdcControlV1 + ?Sized,
{
    let result = update_file_inner(
        path,
        mode,
        base,
        range,
        inserted_len,
        inserted,
        base_bytes,
        evidence,
        objects,
        output,
        buffers,
        control,
        UpdateMemoryAdmissionV1::Borrowed(reservation),
        counters,
    );
    if result.is_err() {
        let _ = counters.add(CounterFieldV1::UpdateFailures, 1);
    }
    result
}

#[derive(Clone, Copy)]
enum UpdateMemoryAdmissionV1<'a> {
    Independent(&'a ResourceLedgerV1),
    Borrowed(&'a OperationReservationV1<'a>),
}

#[allow(clippy::too_many_arguments)]
fn update_file_inner<S, O, R, E, B, C>(
    path: &[u8],
    mode: u16,
    base: AuthenticatedBaseFileV1,
    range: UpdateRangeV1,
    inserted_len: u64,
    inserted: &mut S,
    base_bytes: &mut B,
    evidence: &mut E,
    objects: &mut O,
    output: &mut R,
    buffers: UpdateBuffersV1<'_>,
    control: &mut C,
    admission: UpdateMemoryAdmissionV1<'_>,
    counters: &mut OperationCountersV1,
) -> CoreResult<PreparedFileV1>
where
    S: ContentSourceV1 + ?Sized,
    O: PreparedObjectSinkV1 + ?Sized,
    R: ChunkReferenceSpoolV1 + ?Sized,
    E: BaseChunkEvidenceSourceV1 + ?Sized,
    B: AuthenticatedBaseByteReaderV1 + ?Sized,
    C: CdcControlV1 + ?Sized,
{
    ValidatedPath::new(path)?;
    validate_file_mode(mode)?;
    validate_logical_length(inserted_len)?;
    let base_len = base.identity.logical_len();
    if range.end > base_len || range.start > range.end {
        return Err(CoreError::RangeResyncFailed);
    }
    let new_len = base_len
        .checked_sub(range.len())
        .and_then(|len| len.checked_add(inserted_len))
        .ok_or(CoreError::RangeResyncFailed)?;
    validate_logical_length(new_len).map_err(|_| CoreError::RangeResyncFailed)?;
    let maximum_refs = new_len
        .checked_add(8_191)
        .ok_or(CoreError::RangeResyncFailed)?
        / 8_192;
    validate_chunk_refs_per_file(maximum_refs).map_err(|_| CoreError::RangeResyncFailed)?;
    let metadata_bytes = output
        .resident_memory_bound_bytes(maximum_refs)
        .map_err(|_| CoreError::RangeResyncFailed)?;
    let source_bytes = inserted
        .resident_memory_bound_bytes()
        .map_err(|_| CoreError::RangeResyncFailed)?;
    let object_sink_bytes = objects
        .resident_memory_bound_bytes()
        .map_err(|_| CoreError::RangeResyncFailed)?;
    let evidence_bytes = evidence
        .resident_memory_bound_bytes()
        .map_err(|_| CoreError::RangeResyncFailed)?;
    let base_reader_bytes = base_bytes
        .resident_memory_bound_bytes()
        .map_err(|_| CoreError::RangeResyncFailed)?;
    let operation_metadata_bytes = metadata_bytes
        .checked_add(source_bytes)
        .and_then(|bytes| bytes.checked_add(object_sink_bytes))
        .and_then(|bytes| bytes.checked_add(evidence_bytes))
        .and_then(|bytes| bytes.checked_add(base_reader_bytes))
        .ok_or(CoreError::RangeResyncFailed)?;

    let memory = OperationMemoryPlanV1::empty()
        .charge(MemoryComponentV1::SourceWindow, buffers.source.len() as u64)?
        .charge(MemoryComponentV1::CdcRing, buffers.cdc_ring.len() as u64)?
        .charge(
            MemoryComponentV1::HashState,
            IDENTITY_HASHER_BYTES_V1
                .checked_mul(2)
                .ok_or(CoreError::IntegerOverflow)?,
        )?
        .charge(MemoryComponentV1::MetadataWindow, operation_metadata_bytes)
        .map_err(|_| CoreError::RangeResyncFailed)?;
    let _independent_reservation: Option<OperationReservationV1<'_>> = match admission {
        UpdateMemoryAdmissionV1::Independent(ledger) => {
            let reservation = ledger.reserve_operation_with_plan(memory)?;
            counters.memory_high_water = counters.memory_high_water.max(ledger.high_water_bytes());
            Some(reservation)
        }
        UpdateMemoryAdmissionV1::Borrowed(reservation) => {
            reservation.require(memory)?;
            None
        }
    };
    authenticate_base_file_evidence_v1(base, evidence, counters)?;

    let predecessor = if base_len == 0 {
        None
    } else {
        let predecessor = evidence
            .containing(range.start, range.start == base_len)
            .map_err(map_evidence)?
            .ok_or(CoreError::RangeResyncFailed)?;
        counters.record_update_reference_metadata(1, CHUNK_REFERENCE_METADATA_BYTES)?;
        Some(predecessor)
    };
    let predecessor_start = predecessor.map_or(0, BaseChunkEvidenceV1::start);
    let prefix_len = range
        .start
        .checked_sub(predecessor_start)
        .ok_or(CoreError::RangeResyncFailed)?;
    if prefix_len > MAXIMUM_CHUNK_BYTES as u64 {
        return Err(CoreError::RangeResyncFailed);
    }

    objects.begin_closure().map_err(map_sink)?;
    if let Err(error) = output.begin(maximum_refs).map_err(map_sink) {
        objects.abort_closure();
        return Err(error);
    }
    let operation_binding = RejoinOperationBindingV1::frozen_fast();
    let result = (|| {
        let prefix_chunk_count =
            copy_untouched_prefix(evidence, output, predecessor_start, counters)?;
        let suffix_origin = prefix_len
            .checked_add(inserted_len)
            .ok_or(CoreError::RangeResyncFailed)?;
        let (rejoin, changed_chunk_count) = {
            let mut consumer = UpdateConsumerV1 {
                evidence,
                objects,
                output,
                counters,
                base_bytes,
                suffix_origin,
                base_suffix_origin: range.end,
                chunk_count: prefix_chunk_count,
                operation_binding: &operation_binding,
                rejoin: None,
                failure: None,
            };
            let mut stream = FastCdcV1::new().stream(buffers.cdc_ring, control)?;

            if prefix_len != 0 {
                let prefix = usize::try_from(prefix_len).map_err(|_| CoreError::IntegerOverflow)?;
                read_base_exact(
                    consumer.base_bytes,
                    predecessor_start,
                    &mut buffers.source[..prefix],
                    consumer.counters,
                    false,
                )?;
                push_update(
                    &mut stream,
                    &buffers.source[..prefix],
                    control,
                    &mut consumer,
                )?;
            }

            let mut inserted_remaining = inserted_len;
            while inserted_remaining != 0 {
                let request = usize::try_from(inserted_remaining.min(MAXIMUM_CHUNK_BYTES as u64))
                    .map_err(|_| CoreError::IntegerOverflow)?;
                consumer.counters.add(CounterFieldV1::SourceReadCalls, 1)?;
                let read = inserted
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
                consumer.counters.record_update_inserted(read_u64)?;
                inserted_remaining = inserted_remaining
                    .checked_sub(read_u64)
                    .ok_or(CoreError::TrailingBytes)?;
                push_update(&mut stream, &buffers.source[..read], control, &mut consumer)?;
            }
            consumer.counters.add(CounterFieldV1::SourceReadCalls, 1)?;
            let probe = inserted
                .read(&mut buffers.source[..1])
                .map_err(|ContentSourceErrorV1::Failure| CoreError::SourceFailure)?;
            if probe != 0 {
                consumer.counters.record_source_bytes_read(1)?;
                return Err(CoreError::TrailingBytes);
            }

            let reached_base_end = crate::cdc::resynchronize_update_v1(
                base_len,
                range.end,
                &mut buffers.source[..],
                |base_cursor, remaining_window, source| {
                    let containing = consumer
                        .evidence
                        .containing(base_cursor, false)
                        .map_err(map_evidence)?
                        .ok_or(CoreError::RangeResyncFailed)?;
                    consumer
                        .counters
                        .record_update_reference_metadata(1, CHUNK_REFERENCE_METADATA_BYTES)?;
                    let segment_end = containing.end()?;
                    let segment_len = segment_end
                        .checked_sub(base_cursor)
                        .ok_or(CoreError::RangeResyncFailed)?;
                    let read_len = segment_len.min(remaining_window);
                    let segment =
                        usize::try_from(read_len).map_err(|_| CoreError::IntegerOverflow)?;
                    read_base_exact(
                        consumer.base_bytes,
                        base_cursor,
                        &mut source[..segment],
                        consumer.counters,
                        true,
                    )?;
                    let consumed = push_update_until_pause(
                        &mut stream,
                        &source[..segment],
                        control,
                        &mut consumer,
                    )?;
                    if consumer.rejoin.is_none() && consumed != segment {
                        return Err(CoreError::RangeResyncFailed);
                    }
                    Ok((read_len, consumer.rejoin.is_some()))
                },
            )?;

            let rejoin = if let Some(rejoin) = consumer.rejoin.take() {
                stream.finish_at_accepted_boundary(control)?;
                Some(rejoin)
            } else if reached_base_end {
                finish_update(&mut stream, control, &mut consumer)?;
                None
            } else {
                return Err(CoreError::RangeResyncFailed);
            };
            (rejoin, consumer.chunk_count)
        };

        let suffix_chunk_count = if let Some(rejoin) = rejoin {
            let rejoin = rejoin.consume(&operation_binding)?;
            copy_untouched_suffix(evidence, output, rejoin.end()?, counters)?
        } else {
            0
        };
        let chunk_count = changed_chunk_count
            .checked_add(suffix_chunk_count)
            .ok_or(CoreError::IntegerOverflow)?;
        let (logical_file, physical_file) =
            write_file_object_and_logical(objects, output, mode, new_len, chunk_count, counters)?;
        let prepared = PreparedFileV1::new(
            logical_file,
            physical_file,
            u32::try_from(chunk_count).map_err(|_| CoreError::IntegerOverflow)?,
        );
        objects.finish_closure(prepared).map_err(map_sink)?;
        Ok(prepared)
    })();

    if result.is_err() {
        output.abort();
        objects.abort_closure();
    }
    result
}

pub(crate) fn authenticate_base_file_evidence_v1<E: BaseChunkEvidenceSourceV1 + ?Sized>(
    base: AuthenticatedBaseFileV1,
    evidence: &mut E,
    counters: &mut OperationCountersV1,
) -> CoreResult<()> {
    validate_file_mode(base.mode).map_err(|_| CoreError::RangeResyncFailed)?;
    evidence.rewind().map_err(map_evidence)?;
    let mut logical_hasher =
        LogicalFileHasherV1::new(base.identity.logical_len(), u64::from(base.chunk_count))
            .map_err(|_| CoreError::RangeResyncFailed)?;
    let mut encoder = CanonicalFileObjectEncoderV1::new(
        base.mode,
        base.identity.logical_len(),
        u64::from(base.chunk_count),
    )
    .map_err(|_| CoreError::RangeResyncFailed)?;
    let mut discard = |_bytes: &[u8]| Ok(());
    encoder
        .begin(&mut discard)
        .map_err(|_| CoreError::RangeResyncFailed)?;
    let mut expected_start = 0_u64;
    for _ in 0..base.chunk_count {
        let chunk = evidence
            .next()
            .map_err(map_evidence)?
            .ok_or(CoreError::RangeResyncFailed)?;
        counters.record_update_reference_metadata(1, CHUNK_REFERENCE_METADATA_BYTES)?;
        validate_chunk_reference_len(u64::from(chunk.len))
            .map_err(|_| CoreError::RangeResyncFailed)?;
        if chunk.start != expected_start {
            return Err(CoreError::RangeResyncFailed);
        }
        expected_start = chunk.end()?;
        logical_hasher
            .push(chunk.prepared().logical_ref())
            .map_err(|_| CoreError::RangeResyncFailed)?;
        encoder
            .emit_chunk_reference(chunk.len, &chunk.physical_id, &mut discard)
            .map_err(|_| CoreError::RangeResyncFailed)?;
    }
    let logical = logical_hasher
        .finish()
        .map_err(|_| CoreError::RangeResyncFailed)?;
    let physical = encoder.finish().map_err(|_| CoreError::RangeResyncFailed)?;
    if evidence.next().map_err(map_evidence)?.is_some()
        || expected_start != base.identity.logical_len()
        || logical != base.identity
        || physical != base.physical_file
    {
        return Err(CoreError::RangeResyncFailed);
    }
    Ok(())
}

/// Re-encode a file object with new metadata while replaying only the already
/// authenticated bounded chunk-reference stream. This borrows the outer root
/// reservation; it cannot mint an independent operation or read base payload.
#[cfg(feature = "operation-polymorphism")]
pub(crate) fn reencode_file_metadata_borrowed_v1<O, R, E>(
    new_mode: u16,
    base: AuthenticatedBaseFileV1,
    evidence: &mut E,
    objects: &mut O,
    output: &mut R,
    _reservation: &OperationReservationV1<'_>,
    counters: &mut OperationCountersV1,
) -> CoreResult<PreparedFileV1>
where
    O: PreparedObjectSinkV1 + ?Sized,
    R: ChunkReferenceSpoolV1 + ?Sized,
    E: BaseChunkEvidenceSourceV1 + ?Sized,
{
    validate_file_mode(new_mode)?;
    objects.begin_closure().map_err(map_sink)?;
    if let Err(error) = output.begin(u64::from(base.chunk_count)).map_err(map_sink) {
        objects.abort_closure();
        return Err(error);
    }

    let result = (|| {
        evidence.rewind().map_err(map_evidence)?;
        let mut expected_start = 0_u64;
        for _ in 0..base.chunk_count {
            let chunk = evidence
                .next()
                .map_err(map_evidence)?
                .ok_or(CoreError::RangeResyncFailed)?;
            counters.record_update_reference_metadata(1, CHUNK_REFERENCE_METADATA_BYTES)?;
            validate_chunk_reference_len(u64::from(chunk.len))?;
            if chunk.start != expected_start {
                return Err(CoreError::RangeResyncFailed);
            }
            expected_start = chunk.end()?;
            output.push(chunk.prepared()).map_err(map_sink)?;
        }
        if evidence.next().map_err(map_evidence)?.is_some()
            || expected_start != base.identity.logical_len()
        {
            return Err(CoreError::RangeResyncFailed);
        }

        let (logical_file, physical_file) = write_file_object_and_logical(
            objects,
            output,
            new_mode,
            base.identity.logical_len(),
            u64::from(base.chunk_count),
            counters,
        )?;
        if logical_file != base.identity {
            return Err(CoreError::IdMismatch);
        }
        let prepared = PreparedFileV1::new(logical_file, physical_file, base.chunk_count);
        objects.finish_closure(prepared).map_err(map_sink)?;
        Ok(prepared)
    })();

    if result.is_err() {
        output.abort();
        objects.abort_closure();
    }
    result
}

fn copy_untouched_prefix<E, R>(
    evidence: &mut E,
    output: &mut R,
    predecessor_start: u64,
    counters: &mut OperationCountersV1,
) -> CoreResult<u64>
where
    E: BaseChunkEvidenceSourceV1 + ?Sized,
    R: ChunkReferenceSpoolV1 + ?Sized,
{
    evidence.rewind().map_err(map_evidence)?;
    let mut copied = 0_u64;
    while let Some(chunk) = evidence.next().map_err(map_evidence)? {
        counters.record_update_reference_metadata(1, CHUNK_REFERENCE_METADATA_BYTES)?;
        if chunk.start == predecessor_start {
            break;
        }
        if chunk.end()? > predecessor_start {
            return Err(CoreError::RangeResyncFailed);
        }
        reuse_ref(output, chunk, counters)?;
        copied = copied.checked_add(1).ok_or(CoreError::IntegerOverflow)?;
    }
    Ok(copied)
}

fn copy_untouched_suffix<E, R>(
    evidence: &mut E,
    output: &mut R,
    suffix_start: u64,
    counters: &mut OperationCountersV1,
) -> CoreResult<u64>
where
    E: BaseChunkEvidenceSourceV1 + ?Sized,
    R: ChunkReferenceSpoolV1 + ?Sized,
{
    evidence.rewind().map_err(map_evidence)?;
    let mut found = false;
    let mut copied = 0_u64;
    let mut final_end = 0_u64;
    while let Some(chunk) = evidence.next().map_err(map_evidence)? {
        counters.record_update_reference_metadata(1, CHUNK_REFERENCE_METADATA_BYTES)?;
        final_end = chunk.end()?;
        if chunk.start >= suffix_start {
            if chunk.start != suffix_start {
                return Err(CoreError::RangeResyncFailed);
            }
            reuse_ref(output, chunk, counters)?;
            copied = copied.checked_add(1).ok_or(CoreError::IntegerOverflow)?;
            found = true;
            break;
        }
    }
    if !found && final_end != suffix_start {
        return Err(CoreError::RangeResyncFailed);
    }
    while let Some(chunk) = evidence.next().map_err(map_evidence)? {
        counters.record_update_reference_metadata(1, CHUNK_REFERENCE_METADATA_BYTES)?;
        reuse_ref(output, chunk, counters)?;
        copied = copied.checked_add(1).ok_or(CoreError::IntegerOverflow)?;
    }
    Ok(copied)
}

fn reuse_ref<R: ChunkReferenceSpoolV1 + ?Sized>(
    output: &mut R,
    chunk: BaseChunkEvidenceV1,
    counters: &mut OperationCountersV1,
) -> CoreResult<()> {
    output.push(chunk.prepared()).map_err(map_sink)?;
    counters.add(
        CounterFieldV1::BytesStructurallyReused,
        u64::from(chunk.len),
    )?;
    counters.add(CounterFieldV1::LogicalChunksReused, 1)?;
    counters.add(CounterFieldV1::PhysicalObjectsReused, 1)
}

struct UpdateConsumerV1<'a, 'operation, E: ?Sized, O: ?Sized, R: ?Sized, B: ?Sized> {
    evidence: &'a mut E,
    objects: &'a mut O,
    output: &'a mut R,
    counters: &'a mut OperationCountersV1,
    base_bytes: &'a mut B,
    suffix_origin: u64,
    base_suffix_origin: u64,
    chunk_count: u64,
    operation_binding: &'operation RejoinOperationBindingV1,
    rejoin: Option<VerifiedRejoinV1<'operation, BaseChunkEvidenceV1>>,
    failure: Option<CoreError>,
}

impl<E, O, R, B> BoundaryConsumerV1 for UpdateConsumerV1<'_, '_, E, O, R, B>
where
    E: BaseChunkEvidenceSourceV1 + ?Sized,
    O: PreparedObjectSinkV1 + ?Sized,
    R: ChunkReferenceSpoolV1 + ?Sized,
    B: AuthenticatedBaseByteReaderV1 + ?Sized,
{
    fn accept(
        &mut self,
        boundary: ChunkBoundaryV1,
        chunk: BorrowedChunkV1<'_>,
    ) -> Result<(), CdcBoundaryConsumerErrorV1> {
        if let Err(error) = self.accept_inner(boundary, chunk) {
            self.failure = Some(error);
            Err(CdcBoundaryConsumerErrorV1::Refused)
        } else {
            Ok(())
        }
    }

    fn pause_after_accepted_boundary(&self) -> bool {
        self.rejoin.is_some()
    }
}

impl<E, O, R, B> UpdateConsumerV1<'_, '_, E, O, R, B>
where
    E: BaseChunkEvidenceSourceV1 + ?Sized,
    O: PreparedObjectSinkV1 + ?Sized,
    R: ChunkReferenceSpoolV1 + ?Sized,
    B: AuthenticatedBaseByteReaderV1 + ?Sized,
{
    fn accept_inner(
        &mut self,
        boundary: ChunkBoundaryV1,
        chunk: BorrowedChunkV1<'_>,
    ) -> CoreResult<()> {
        if self.rejoin.is_some() {
            return Err(CoreError::RangeResyncFailed);
        }
        let chunk_len = u64::try_from(chunk.len()).map_err(|_| CoreError::IntegerOverflow)?;
        self.counters
            .add(CounterFieldV1::LogicalHashBytes, chunk_len)?;
        self.counters.add(
            CounterFieldV1::LogicalHashUpdateCalls,
            u64::from(!chunk.first().is_empty()) + u64::from(!chunk.second().is_empty()),
        )?;
        let logical = derive_logical_chunk_spans_v1(chunk.first(), chunk.second())?;
        if boundary.start() >= self.suffix_origin {
            let base_start = self
                .base_suffix_origin
                .checked_add(boundary.start() - self.suffix_origin)
                .ok_or(CoreError::RangeResyncFailed)?;
            self.counters.add(CounterFieldV1::AnchorAttempts, 1)?;
            if let Some(base) = self.evidence.at_start(base_start).map_err(map_evidence)? {
                self.counters
                    .record_update_reference_metadata(1, CHUNK_REFERENCE_METADATA_BYTES)?;
                let rejoin =
                    if u64::from(base.len) == boundary.len() && base.logical_id == logical.id() {
                        crate::cdc::verify_rejoin_bytes_v1(
                            self.operation_binding,
                            base,
                            base.start(),
                            base.len(),
                            chunk,
                            self.counters,
                            |offset, first, second| {
                                self.base_bytes
                                    .compare_exact_at(offset, first, second)
                                    .map_err(|_| CoreError::RangeResyncFailed)
                            },
                        )?
                    } else {
                        None
                    };
                if let Some(rejoin) = rejoin {
                    reuse_ref(self.output, base, self.counters)?;
                    self.chunk_count = self
                        .chunk_count
                        .checked_add(1)
                        .ok_or(CoreError::IntegerOverflow)?;
                    self.rejoin = Some(rejoin);
                    return Ok(());
                }
            }
        }

        let (physical_id, disposition) = write_chunk_object(self.objects, chunk, self.counters)?;
        self.output
            .push(PreparedChunkRefV1::from_parts(
                logical.id(),
                physical_id,
                u32::try_from(chunk.len()).map_err(|_| CoreError::IntegerOverflow)?,
            ))
            .map_err(map_sink)?;
        self.chunk_count = self
            .chunk_count
            .checked_add(1)
            .ok_or(CoreError::IntegerOverflow)?;
        validate_chunk_refs_per_file(self.chunk_count)?;
        match disposition {
            ObjectDispositionV1::Created => {
                self.counters.add(CounterFieldV1::LogicalChunksCreated, 1)
            }
            ObjectDispositionV1::Reused => {
                self.counters.add(CounterFieldV1::LogicalChunksReused, 1)
            }
        }
    }
}

fn push_update<C, E, O, R, B>(
    stream: &mut crate::cdc::FastCdcV1Stream<'_>,
    bytes: &[u8],
    control: &mut C,
    consumer: &mut UpdateConsumerV1<'_, '_, E, O, R, B>,
) -> CoreResult<()>
where
    C: CdcControlV1 + ?Sized,
    E: BaseChunkEvidenceSourceV1 + ?Sized,
    O: PreparedObjectSinkV1 + ?Sized,
    R: ChunkReferenceSpoolV1 + ?Sized,
    B: AuthenticatedBaseByteReaderV1 + ?Sized,
{
    let before = stream.counters();
    let result = match stream.push(Ok(bytes), control, consumer) {
        Ok(()) => Ok(()),
        Err(error) => Err(consumer.failure.take().unwrap_or(error)),
    };
    consumer
        .counters
        .add_cdc_stream(stream.counters().checked_delta(before)?)?;
    result
}

fn push_update_until_pause<C, E, O, R, B>(
    stream: &mut crate::cdc::FastCdcV1Stream<'_>,
    bytes: &[u8],
    control: &mut C,
    consumer: &mut UpdateConsumerV1<'_, '_, E, O, R, B>,
) -> CoreResult<usize>
where
    C: CdcControlV1 + ?Sized,
    E: BaseChunkEvidenceSourceV1 + ?Sized,
    O: PreparedObjectSinkV1 + ?Sized,
    R: ChunkReferenceSpoolV1 + ?Sized,
    B: AuthenticatedBaseByteReaderV1 + ?Sized,
{
    let before = stream.counters();
    let result = match stream.push_until_consumer_pause(Ok(bytes), control, consumer) {
        Ok(consumed) => Ok(consumed),
        Err(error) => Err(consumer.failure.take().unwrap_or(error)),
    };
    consumer
        .counters
        .add_cdc_stream(stream.counters().checked_delta(before)?)?;
    result
}

fn finish_update<C, E, O, R, B>(
    stream: &mut crate::cdc::FastCdcV1Stream<'_>,
    control: &mut C,
    consumer: &mut UpdateConsumerV1<'_, '_, E, O, R, B>,
) -> CoreResult<()>
where
    C: CdcControlV1 + ?Sized,
    E: BaseChunkEvidenceSourceV1 + ?Sized,
    O: PreparedObjectSinkV1 + ?Sized,
    R: ChunkReferenceSpoolV1 + ?Sized,
    B: AuthenticatedBaseByteReaderV1 + ?Sized,
{
    let before = stream.counters();
    let result = match stream.finish(control, consumer) {
        Ok(()) => Ok(()),
        Err(error) => Err(consumer.failure.take().unwrap_or(error)),
    };
    consumer
        .counters
        .add_cdc_stream(stream.counters().checked_delta(before)?)?;
    result
}

fn read_base_exact<B: AuthenticatedBaseByteReaderV1 + ?Sized>(
    base: &mut B,
    offset: u64,
    destination: &mut [u8],
    counters: &mut OperationCountersV1,
    resynchronization: bool,
) -> CoreResult<()> {
    base.read_exact_at(offset, destination)
        .map_err(|_| CoreError::RangeResyncFailed)?;
    let len = u64::try_from(destination.len()).map_err(|_| CoreError::IntegerOverflow)?;
    counters.add(CounterFieldV1::BytesRead, len)?;
    counters.add(CounterFieldV1::BytesCopied, len)?;
    counters.record_update_base_payload(len)?;
    if resynchronization {
        counters.add(CounterFieldV1::UpdateResynchronizationBytes, len)?;
    }
    Ok(())
}

fn map_sink(PreparedSinkErrorV1::Refused: PreparedSinkErrorV1) -> CoreError {
    CoreError::SinkRefused
}

fn map_evidence(PreparedSinkErrorV1::Refused: PreparedSinkErrorV1) -> CoreError {
    CoreError::RangeResyncFailed
}
