//! One-pass Create, explicit Replace, and bounded Update construction.
//!
//! File-object preparation is owned by `file`; complete root orchestration,
//! read verification, Replace, and Update retain their separate semantic owners.

#[cfg(feature = "operation-polymorphism")]
mod create;
mod file;
mod read;
mod replace;
pub(crate) mod update;

pub(crate) use file::*;
pub(crate) use read::{
    stream_verified_file_range_v1, VerifiedFileBytesConsumerV1, VerifiedFileRangePortV1,
    VerifiedFileSegmentV1,
};

#[cfg(feature = "operation-polymorphism")]
pub(crate) use crate::lifecycle::{request_create_operation_v1, request_tree_operation_v1};

#[cfg(feature = "operation-polymorphism")]
pub(crate) use crate::lifecycle::{run_create_tree_v1, run_create_v1};
#[cfg(feature = "operation-polymorphism")]
pub(crate) use crate::lifecycle::{OperationBuffersV1, OperationErrorV1};
#[cfg(feature = "operation-polymorphism")]
pub(crate) use create::{SourceSupplierV1, TreeFileV1};
#[cfg(feature = "operation-polymorphism")]
pub(crate) use replace::replace_file_borrowed_v1;
pub(crate) use replace::replace_file_v1;

/// Small semantic observations for the default content-owner tests.  The
/// adapter owns the private source/sink/spool plumbing; callers can request a
/// bounded content scenario and observe only typed outcomes and counters.
pub mod semantic {
    use super::{
        create_file_v1, replace_file_v1, ChunkReferenceSpoolV1, ContentBuffersV1,
        ContentSourceErrorV1, ContentSourceV1, ObjectDispositionV1, PreparedChunkRefV1,
        PreparedFileV1, PreparedObjectSinkV1, PreparedSinkErrorV1,
    };
    use crate::cdc::ContinueCdcControlV1;
    use crate::identity::DIGEST_BYTES;
    use crate::limits::{
        OperationCountersV1, ResourceLedgerV1, BASE_LEDGER_BYTES, MEMORY_PROFILE_32_MIB,
        OPERATION_SLOT_BYTES,
    };
    use crate::object::{decode_physical_object_v1, DiscardStrongEdgesV1, TypedPhysicalObjectIdV1};
    use crate::{CoreError, CoreResult};

    pub use super::update::semantic::{
        expected_planned_high_water, first_chunk_end, max_update_resynchronization_bytes,
        update_from_reader_v1, update_v1, UpdateObservationV1, UpdateRequestV1,
    };

    const BUFFER_BYTES: usize = crate::cdc::MAXIMUM_CHUNK_BYTES;

    #[derive(Clone, Copy, Debug)]
    pub struct ContentRequestV1<'a> {
        path: &'a [u8],
        mode: u16,
        data: &'a [u8],
        declared_len: u64,
        budget_bytes: u64,
        source_resident_bytes: u64,
        sink_resident_bytes: u64,
        spool_resident_bytes: Option<u64>,
        sink_refuse_after: Option<u64>,
        invalid_source_count: bool,
    }

    impl<'a> ContentRequestV1<'a> {
        pub const fn new(path: &'a [u8], mode: u16, data: &'a [u8]) -> Self {
            Self {
                path,
                mode,
                data,
                declared_len: data.len() as u64,
                budget_bytes: MEMORY_PROFILE_32_MIB,
                source_resident_bytes: 0,
                sink_resident_bytes: 0,
                spool_resident_bytes: None,
                sink_refuse_after: None,
                invalid_source_count: false,
            }
        }

        pub const fn with_declared_len(mut self, declared_len: u64) -> Self {
            self.declared_len = declared_len;
            self
        }

        pub const fn with_budget(mut self, budget_bytes: u64) -> Self {
            self.budget_bytes = budget_bytes;
            self
        }

        pub const fn with_source_residency(mut self, bytes: u64) -> Self {
            self.source_resident_bytes = bytes;
            self
        }

        pub const fn with_sink_residency(mut self, bytes: u64) -> Self {
            self.sink_resident_bytes = bytes;
            self
        }

        pub const fn with_spool_residency(mut self, bytes: u64) -> Self {
            self.spool_resident_bytes = Some(bytes);
            self
        }

        pub const fn with_sink_refusal_after(mut self, bytes: u64) -> Self {
            self.sink_refuse_after = Some(bytes);
            self
        }

        pub const fn with_invalid_source_count(mut self, invalid: bool) -> Self {
            self.invalid_source_count = invalid;
            self
        }
    }

    pub const fn base_budget_bytes() -> u64 {
        BASE_LEDGER_BYTES
    }

    pub const fn operation_slot_bytes() -> u64 {
        OPERATION_SLOT_BYTES
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub struct CreateObservationV1 {
        logical_id: [u8; DIGEST_BYTES],
        physical_id: [u8; DIGEST_BYTES],
        bytes_read: u64,
        source_remaining: u64,
        source_read_calls: u64,
        bytes_copied: u64,
        ring_fills: u64,
        ring_wrap_spans: u64,
        cdc_scan_calls: u64,
        cdc_scan_bytes: u64,
        bytes_boundary_inspected: u64,
        logical_hash_bytes: u64,
        logical_hash_update_calls: u64,
        physical_hash_bytes: u64,
        physical_hash_update_calls: u64,
        logical_chunks_reused: u64,
        physical_objects_reused: u64,
        source_max_request: u64,
        sink_max_segment: u64,
        object_count: u64,
        file_object_observed: bool,
        memory_high_water: u64,
        ledger_high_water: u64,
        planned_high_water: u64,
        logical_len: u64,
        chunk_count: u32,
        file_chunk_ref_count: u64,
        spool_ref_count: u64,
        admitted_slots: u64,
        completed: bool,
        zero_forbidden_work: bool,
    }

    impl CreateObservationV1 {
        pub const fn logical_id(self) -> [u8; DIGEST_BYTES] {
            self.logical_id
        }

        pub const fn physical_id(self) -> [u8; DIGEST_BYTES] {
            self.physical_id
        }

        pub const fn bytes_read(self) -> u64 {
            self.bytes_read
        }

        pub const fn source_remaining(self) -> u64 {
            self.source_remaining
        }

        pub const fn source_read_calls(self) -> u64 {
            self.source_read_calls
        }

        pub const fn bytes_copied(self) -> u64 {
            self.bytes_copied
        }

        pub const fn ring_fills(self) -> u64 {
            self.ring_fills
        }

        pub const fn ring_wrap_spans(self) -> u64 {
            self.ring_wrap_spans
        }

        pub const fn cdc_scan_calls(self) -> u64 {
            self.cdc_scan_calls
        }

        pub const fn cdc_scan_bytes(self) -> u64 {
            self.cdc_scan_bytes
        }

        pub const fn bytes_boundary_inspected(self) -> u64 {
            self.bytes_boundary_inspected
        }

        pub const fn logical_hash_bytes(self) -> u64 {
            self.logical_hash_bytes
        }

        pub const fn logical_hash_update_calls(self) -> u64 {
            self.logical_hash_update_calls
        }

        pub const fn physical_hash_bytes(self) -> u64 {
            self.physical_hash_bytes
        }

        pub const fn physical_hash_update_calls(self) -> u64 {
            self.physical_hash_update_calls
        }

        pub const fn logical_chunks_reused(self) -> u64 {
            self.logical_chunks_reused
        }

        pub const fn physical_objects_reused(self) -> u64 {
            self.physical_objects_reused
        }

        pub const fn source_max_request(self) -> u64 {
            self.source_max_request
        }

        pub const fn sink_max_segment(self) -> u64 {
            self.sink_max_segment
        }

        pub const fn object_count(self) -> u64 {
            self.object_count
        }

        pub const fn file_object_observed(self) -> bool {
            self.file_object_observed
        }

        pub const fn memory_high_water(self) -> u64 {
            self.memory_high_water
        }

        pub const fn ledger_high_water(self) -> u64 {
            self.ledger_high_water
        }

        pub const fn planned_high_water(self) -> u64 {
            self.planned_high_water
        }

        pub const fn logical_len(self) -> u64 {
            self.logical_len
        }

        pub const fn chunk_count(self) -> u32 {
            self.chunk_count
        }

        pub const fn file_chunk_ref_count(self) -> u64 {
            self.file_chunk_ref_count
        }

        pub const fn spool_ref_count(self) -> u64 {
            self.spool_ref_count
        }

        pub const fn admitted_slots(self) -> u64 {
            self.admitted_slots
        }

        pub const fn completed(self) -> bool {
            self.completed
        }

        pub const fn zero_forbidden_work(self) -> bool {
            self.zero_forbidden_work
        }
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub struct FailureObservationV1 {
        error: CoreError,
        source_reads: u64,
        sink_active: bool,
        sink_completed: bool,
        sink_aborts: u64,
        spool_aborted: bool,
        admitted_slots: u64,
        bytes_read: u64,
        bytes_copied: u64,
    }

    impl FailureObservationV1 {
        pub const fn error(self) -> CoreError {
            self.error
        }

        pub const fn source_reads(self) -> u64 {
            self.source_reads
        }

        pub const fn sink_active(self) -> bool {
            self.sink_active
        }

        pub const fn sink_completed(self) -> bool {
            self.sink_completed
        }

        pub const fn sink_aborts(self) -> u64 {
            self.sink_aborts
        }

        pub const fn spool_aborted(self) -> bool {
            self.spool_aborted
        }

        pub const fn admitted_slots(self) -> u64 {
            self.admitted_slots
        }

        pub const fn bytes_read(self) -> u64 {
            self.bytes_read
        }

        pub const fn bytes_copied(self) -> u64 {
            self.bytes_copied
        }
    }

    struct Source<'a> {
        bytes: &'a [u8],
        offset: usize,
        resident_bytes: u64,
        reads: u64,
        invalid_count: bool,
        max_request: u64,
    }

    impl<'a> Source<'a> {
        fn new(bytes: &'a [u8], resident_bytes: u64) -> Self {
            Self {
                bytes,
                offset: 0,
                resident_bytes,
                reads: 0,
                invalid_count: false,
                max_request: 0,
            }
        }

        fn invalid() -> Self {
            Self {
                invalid_count: true,
                ..Self::new(&[], 0)
            }
        }
    }

    impl ContentSourceV1 for Source<'_> {
        fn resident_memory_bound_bytes(&self) -> CoreResult<u64> {
            Ok(self.resident_bytes)
        }

        fn read(&mut self, destination: &mut [u8]) -> Result<usize, ContentSourceErrorV1> {
            self.reads += 1;
            self.max_request = self.max_request.max(destination.len() as u64);
            if self.invalid_count {
                return Ok(destination.len() + 1);
            }
            let amount = destination
                .len()
                .min(self.bytes.len().saturating_sub(self.offset));
            destination[..amount].copy_from_slice(&self.bytes[self.offset..self.offset + amount]);
            self.offset += amount;
            Ok(amount)
        }
    }

    #[derive(Default)]
    struct Spool {
        values: Vec<PreparedChunkRefV1>,
        cursor: usize,
        maximum: u64,
        reported_resident_bytes: Option<u64>,
        aborted: bool,
    }

    impl ChunkReferenceSpoolV1 for Spool {
        fn resident_memory_bound_bytes(&self, maximum_refs: u64) -> CoreResult<u64> {
            self.reported_resident_bytes
                .unwrap_or_else(|| {
                    maximum_refs
                        .checked_mul(core::mem::size_of::<PreparedChunkRefV1>() as u64)
                        .unwrap_or(u64::MAX)
                })
                .try_into()
                .map_err(|_| CoreError::IntegerOverflow)
        }

        fn begin(&mut self, maximum_refs: u64) -> Result<(), PreparedSinkErrorV1> {
            self.values.clear();
            self.cursor = 0;
            self.maximum = maximum_refs;
            self.aborted = false;
            Ok(())
        }

        fn push(&mut self, chunk: PreparedChunkRefV1) -> Result<(), PreparedSinkErrorV1> {
            if self.values.len() as u64 == self.maximum {
                return Err(PreparedSinkErrorV1::Refused);
            }
            self.values.push(chunk);
            Ok(())
        }

        fn rewind(&mut self) -> Result<(), PreparedSinkErrorV1> {
            self.cursor = 0;
            Ok(())
        }

        fn next(&mut self) -> Result<Option<PreparedChunkRefV1>, PreparedSinkErrorV1> {
            let value = self.values.get(self.cursor).copied();
            self.cursor += usize::from(value.is_some());
            Ok(value)
        }

        fn abort(&mut self) {
            self.values.clear();
            self.aborted = true;
        }
    }

    #[derive(Default)]
    struct Sink {
        resident_bytes: u64,
        current: Vec<u8>,
        declared: u64,
        objects: Vec<(TypedPhysicalObjectIdV1, Vec<u8>)>,
        active: bool,
        aborts: u64,
        refuse_after: Option<u64>,
        bytes: u64,
        max_segment: u64,
        completed: Option<PreparedFileV1>,
    }

    impl PreparedObjectSinkV1 for Sink {
        fn resident_memory_bound_bytes(&self) -> CoreResult<u64> {
            Ok(self.resident_bytes)
        }

        fn begin_closure(&mut self) -> Result<(), PreparedSinkErrorV1> {
            self.active = true;
            Ok(())
        }

        fn begin_object(
            &mut self,
            _kind: crate::format::PhysicalObjectKindV1,
            complete_len: u64,
        ) -> Result<(), PreparedSinkErrorV1> {
            self.current.clear();
            self.declared = complete_len;
            Ok(())
        }

        fn write_private(&mut self, bytes: &[u8]) -> Result<(), PreparedSinkErrorV1> {
            let next = self.bytes.saturating_add(bytes.len() as u64);
            if self.refuse_after.is_some_and(|limit| next > limit) {
                return Err(PreparedSinkErrorV1::Refused);
            }
            self.bytes = next;
            self.max_segment = self.max_segment.max(bytes.len() as u64);
            self.current.extend_from_slice(bytes);
            Ok(())
        }

        fn finish_object(
            &mut self,
            expected_id: TypedPhysicalObjectIdV1,
        ) -> Result<ObjectDispositionV1, PreparedSinkErrorV1> {
            if self.current.len() as u64 != self.declared {
                return Err(PreparedSinkErrorV1::Refused);
            }
            let decoded = decode_physical_object_v1(&self.current, &mut DiscardStrongEdgesV1)
                .map_err(|_| PreparedSinkErrorV1::Refused)?;
            if decoded
                .physical_id()
                .map_err(|_| PreparedSinkErrorV1::Refused)?
                != expected_id
            {
                return Err(PreparedSinkErrorV1::Refused);
            }
            let reused = self.objects.iter().any(|(id, bytes)| {
                *id == expected_id && bytes.as_slice() == self.current.as_slice()
            });
            if !reused {
                self.objects.push((expected_id, self.current.clone()));
            }
            Ok(if reused {
                ObjectDispositionV1::Reused
            } else {
                ObjectDispositionV1::Created
            })
        }

        fn finish_closure(&mut self, result: PreparedFileV1) -> Result<(), PreparedSinkErrorV1> {
            self.completed = Some(result);
            self.active = false;
            Ok(())
        }

        fn abort_closure(&mut self) {
            self.aborts += 1;
            self.active = false;
            self.completed = None;
            self.current.clear();
        }
    }

    fn buffers() -> ([u8; BUFFER_BYTES], [u8; BUFFER_BYTES]) {
        ([0; BUFFER_BYTES], [0; BUFFER_BYTES])
    }

    fn snapshot(
        created: PreparedFileV1,
        request: &ContentRequestV1<'_>,
        sink: &Sink,
        spool: &Spool,
        counters: &OperationCountersV1,
        ledger: &ResourceLedgerV1,
        source: &Source<'_>,
    ) -> CreateObservationV1 {
        CreateObservationV1 {
            logical_id: *created.logical_file().id().as_bytes(),
            physical_id: *created.physical_file().as_bytes(),
            bytes_read: counters.bytes_read,
            source_remaining: source.bytes.len().saturating_sub(source.offset) as u64,
            source_read_calls: counters.source_read_calls,
            bytes_copied: counters.bytes_copied,
            ring_fills: counters.ring_fills,
            ring_wrap_spans: counters.ring_wrap_spans,
            cdc_scan_calls: counters.cdc_scan_calls,
            cdc_scan_bytes: counters.cdc_scan_bytes,
            bytes_boundary_inspected: counters.bytes_boundary_inspected,
            logical_hash_bytes: counters.logical_hash_bytes,
            logical_hash_update_calls: counters.logical_hash_update_calls,
            physical_hash_bytes: counters.physical_hash_bytes,
            physical_hash_update_calls: counters.physical_hash_update_calls,
            logical_chunks_reused: counters.logical_chunks_reused,
            physical_objects_reused: counters.physical_objects_reused,
            source_max_request: source.max_request,
            sink_max_segment: sink.max_segment,
            object_count: sink.objects.len() as u64,
            file_object_observed: sink
                .objects
                .iter()
                .any(|(id, _)| matches!(id, TypedPhysicalObjectIdV1::File(_))),
            memory_high_water: counters.memory_high_water,
            ledger_high_water: ledger.high_water_bytes(),
            planned_high_water: ledger.planned_high_water_bytes(),
            logical_len: request.declared_len,
            chunk_count: created.chunk_count(),
            file_chunk_ref_count: u64::from(created.chunk_count()),
            spool_ref_count: spool.values.len() as u64,
            admitted_slots: ledger.admitted_slots(),
            completed: sink.completed.is_some(),
            zero_forbidden_work: counters.has_zero_forbidden_work(),
        }
    }

    pub fn create_and_replace_v1(
        request: &ContentRequestV1<'_>,
    ) -> CoreResult<(CreateObservationV1, CreateObservationV1)> {
        let ledger = ResourceLedgerV1::new(request.budget_bytes);
        let mut sink = Sink::default();
        let mut spool = Spool::default();
        let mut source = Source::new(request.data, request.source_resident_bytes);
        let (mut source_buffer, mut ring) = buffers();
        let mut counters = OperationCountersV1::default();
        let created = create_file_v1(
            request.path,
            request.mode,
            request.declared_len,
            &mut source,
            &mut sink,
            &mut spool,
            ContentBuffersV1::new(&mut source_buffer, &mut ring),
            &mut ContinueCdcControlV1,
            &ledger,
            &mut counters,
        )?;
        let first = snapshot(created, request, &sink, &spool, &counters, &ledger, &source);

        let mut replacement_source = Source::new(request.data, request.source_resident_bytes);
        let (mut replacement_buffer, mut replacement_ring) = buffers();
        let mut replacement_spool = Spool::default();
        let replaced = replace_file_v1(
            request.path,
            request.mode,
            request.declared_len,
            &mut replacement_source,
            &mut sink,
            &mut replacement_spool,
            ContentBuffersV1::new(&mut replacement_buffer, &mut replacement_ring),
            &mut ContinueCdcControlV1,
            &ledger,
            &mut counters,
        )?;
        Ok((
            first,
            snapshot(
                replaced,
                request,
                &sink,
                &replacement_spool,
                &counters,
                &ledger,
                &replacement_source,
            ),
        ))
    }

    pub fn create_v1(request: &ContentRequestV1<'_>) -> CoreResult<CreateObservationV1> {
        let ledger = ResourceLedgerV1::new(request.budget_bytes);
        let mut sink = Sink::default();
        let mut spool = Spool::default();
        let mut source = Source::new(request.data, request.source_resident_bytes);
        let (mut source_buffer, mut ring) = buffers();
        let mut counters = OperationCountersV1::default();
        let created = create_file_v1(
            request.path,
            request.mode,
            request.declared_len,
            &mut source,
            &mut sink,
            &mut spool,
            ContentBuffersV1::new(&mut source_buffer, &mut ring),
            &mut ContinueCdcControlV1,
            &ledger,
            &mut counters,
        )?;
        Ok(snapshot(
            created, request, &sink, &spool, &counters, &ledger, &source,
        ))
    }

    pub fn observe_failure_v1(request: &ContentRequestV1<'_>) -> FailureObservationV1 {
        let ledger = ResourceLedgerV1::new(request.budget_bytes);
        let mut source = if request.invalid_source_count {
            Source::invalid()
        } else {
            Source::new(request.data, request.source_resident_bytes)
        };
        let mut sink = Sink {
            resident_bytes: request.sink_resident_bytes,
            refuse_after: request.sink_refuse_after,
            ..Sink::default()
        };
        let mut spool = Spool {
            reported_resident_bytes: request.spool_resident_bytes,
            ..Spool::default()
        };
        let (mut source_buffer, mut ring) = buffers();
        let mut counters = OperationCountersV1::default();
        let error = create_file_v1(
            request.path,
            request.mode,
            request.declared_len,
            &mut source,
            &mut sink,
            &mut spool,
            ContentBuffersV1::new(&mut source_buffer, &mut ring),
            &mut ContinueCdcControlV1,
            &ledger,
            &mut counters,
        )
        .unwrap_err();
        FailureObservationV1 {
            error,
            source_reads: source.reads,
            sink_active: sink.active,
            sink_completed: sink.completed.is_some(),
            sink_aborts: sink.aborts,
            spool_aborted: spool.aborted,
            admitted_slots: ledger.admitted_slots(),
            bytes_read: counters.bytes_read,
            bytes_copied: counters.bytes_copied,
        }
    }
}
