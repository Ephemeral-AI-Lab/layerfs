mod support;

use layerfs_storage::cdc::{ContinueCdcControlV1, MAXIMUM_CHUNK_BYTES};
use layerfs_storage::content::{
    create_file_v1, replace_file_v1, ChunkReferenceSpoolV1, ContentBuffersV1, ContentSourceErrorV1,
    ContentSourceV1, ObjectDispositionV1, PreparedChunkRefV1, PreparedFileV1, PreparedObjectSinkV1,
    PreparedSinkErrorV1,
};
use layerfs_storage::format::PhysicalObjectKindV1;
use layerfs_storage::identity::IDENTITY_HASHER_BYTES_V1;
use layerfs_storage::limits::{
    admitted_slots_for_budget, OperationCountersV1, ResourceLedgerV1, BASE_LEDGER_BYTES,
    OPERATION_SLOT_BYTES,
};
use layerfs_storage::object::{
    decode_physical_object_v1, DiscardStrongEdgesV1, PhysicalObjectPayloadV1,
    TypedPhysicalObjectIdV1,
};
use layerfs_storage::CoreError;

#[derive(Default)]
struct RefSpool {
    values: Vec<PreparedChunkRefV1>,
    cursor: usize,
    maximum: u64,
    aborted: bool,
    reported_resident_bytes: Option<u64>,
}

impl ChunkReferenceSpoolV1 for RefSpool {
    fn resident_memory_bound_bytes(&self, maximum_refs: u64) -> Result<u64, CoreError> {
        if let Some(bytes) = self.reported_resident_bytes {
            return Ok(bytes);
        }
        maximum_refs
            .checked_mul(core::mem::size_of::<PreparedChunkRefV1>() as u64)
            .ok_or(CoreError::IntegerOverflow)
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

struct SliceSource<'a> {
    bytes: &'a [u8],
    offset: usize,
    resident_memory: u64,
    reads: u64,
}

impl<'a> SliceSource<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self {
            bytes,
            offset: 0,
            resident_memory: 0,
            reads: 0,
        }
    }
}

impl ContentSourceV1 for SliceSource<'_> {
    fn resident_memory_bound_bytes(&self) -> Result<u64, CoreError> {
        Ok(self.resident_memory)
    }

    fn read(&mut self, destination: &mut [u8]) -> Result<usize, ContentSourceErrorV1> {
        self.reads += 1;
        let amount = destination.len().min(self.bytes.len() - self.offset);
        destination[..amount].copy_from_slice(&self.bytes[self.offset..self.offset + amount]);
        self.offset += amount;
        Ok(amount)
    }
}

struct InvalidCountSource;

impl ContentSourceV1 for InvalidCountSource {
    fn resident_memory_bound_bytes(&self) -> Result<u64, CoreError> {
        Ok(0)
    }

    fn read(&mut self, destination: &mut [u8]) -> Result<usize, ContentSourceErrorV1> {
        Ok(destination.len() + 1)
    }
}

#[derive(Default)]
struct CapturingSink {
    resident_memory: u64,
    current: Vec<u8>,
    declared: u64,
    objects: Vec<(TypedPhysicalObjectIdV1, Vec<u8>)>,
    completed: Option<PreparedFileV1>,
    active: bool,
    aborts: u64,
    refuse_after: Option<u64>,
    bytes: u64,
}

impl PreparedObjectSinkV1 for CapturingSink {
    fn resident_memory_bound_bytes(&self) -> Result<u64, CoreError> {
        Ok(self.resident_memory)
    }

    fn begin_closure(&mut self) -> Result<(), PreparedSinkErrorV1> {
        self.active = true;
        Ok(())
    }

    fn begin_object(
        &mut self,
        _kind: PhysicalObjectKindV1,
        complete_len: u64,
    ) -> Result<(), PreparedSinkErrorV1> {
        self.current.clear();
        self.declared = complete_len;
        Ok(())
    }

    fn write_private(&mut self, bytes: &[u8]) -> Result<(), PreparedSinkErrorV1> {
        let next = self.bytes + bytes.len() as u64;
        if self.refuse_after.is_some_and(|limit| next > limit) {
            return Err(PreparedSinkErrorV1::Refused);
        }
        self.bytes = next;
        self.current.extend_from_slice(bytes);
        Ok(())
    }

    fn finish_object(
        &mut self,
        expected_id: TypedPhysicalObjectIdV1,
    ) -> Result<ObjectDispositionV1, PreparedSinkErrorV1> {
        assert_eq!(self.current.len() as u64, self.declared);
        let decoded = decode_physical_object_v1(&self.current, &mut DiscardStrongEdgesV1).unwrap();
        assert_eq!(decoded.physical_id().unwrap(), expected_id);
        let reused = self
            .objects
            .iter()
            .any(|(id, bytes)| *id == expected_id && bytes.as_slice() == self.current.as_slice());
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
        self.objects.clear();
        self.current.clear();
    }
}

fn buffers() -> ([u8; MAXIMUM_CHUNK_BYTES], [u8; MAXIMUM_CHUNK_BYTES]) {
    ([0; MAXIMUM_CHUNK_BYTES], [0; MAXIMUM_CHUNK_BYTES])
}

fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for &byte in bytes {
        output.push(char::from(DIGITS[usize::from(byte >> 4)]));
        output.push(char::from(DIGITS[usize::from(byte & 0x0f)]));
    }
    output
}

#[test]
fn create_and_explicit_replace_produce_exact_prepared_closures() {
    let data = support::fastcdc_golden_input(100_000);
    let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
    let mut sink = CapturingSink::default();
    let mut spool = RefSpool::default();
    let mut source = SliceSource::new(&data);
    let (mut source_buffer, mut ring) = buffers();
    let mut counters = OperationCountersV1::default();
    let created = create_file_v1(
        b"dir/file.bin",
        0o644,
        data.len() as u64,
        &mut source,
        &mut sink,
        &mut spool,
        ContentBuffersV1::new(&mut source_buffer, &mut ring),
        &mut ContinueCdcControlV1,
        &ledger,
        &mut counters,
    )
    .unwrap();
    assert_eq!(sink.completed, Some(created));
    assert_eq!(source.offset, data.len());
    assert_eq!(counters.bytes_read, data.len() as u64);
    assert_eq!(counters.source_read_calls, 5);
    assert_eq!(counters.bytes_copied, data.len() as u64);
    // Four non-empty source fragments cross the circular-ring end three times.
    // The work counters report actual contiguous writes/scans, not source reads.
    assert_eq!(counters.ring_fills, 6);
    assert_eq!(counters.ring_wrap_spans, 3);
    assert_eq!(counters.cdc_scan_calls, 11);
    assert_eq!(counters.cdc_scan_bytes, data.len() as u64);
    assert!(counters.bytes_boundary_inspected > 0);
    assert!(counters.bytes_boundary_inspected <= counters.cdc_scan_bytes);
    assert_eq!(counters.logical_hash_bytes, data.len() as u64);
    assert!(counters.logical_hash_update_calls > 0);
    assert!(counters.physical_hash_bytes > data.len() as u64);
    assert!(counters.physical_hash_update_calls > 0);
    assert!(counters.has_zero_forbidden_work());
    assert_eq!(ledger.admitted_slots(), 0);
    assert_eq!(created.chunk_count() as usize, spool.values.len());

    // Fixed deterministic vectors over the frozen SHA-256 CDC corpus.
    assert_eq!(
        hex(created.logical_file().id().as_bytes()),
        "6a3e35920006d8c817c8c526cf523b2b4ce676f01197587d0f5847414028c103"
    );
    assert_eq!(
        hex(created.physical_file().as_bytes()),
        "e033f4a110b2b8e3e797a7b9bfa5958b1128756298d0b9de43e160b7f73998fb"
    );

    let file_object = sink
        .objects
        .iter()
        .find(|(id, _)| matches!(id, TypedPhysicalObjectIdV1::File(_)))
        .unwrap();
    let decoded = decode_physical_object_v1(&file_object.1, &mut DiscardStrongEdgesV1).unwrap();
    let PhysicalObjectPayloadV1::File(file) = decoded.payload() else {
        panic!("expected File object")
    };
    assert_eq!(file.logical_len, data.len() as u64);
    assert_eq!(file.chunk_ref_count, u64::from(created.chunk_count()));

    let mut replacement_source = SliceSource::new(&data);
    let (mut replacement_buffer, mut replacement_ring) = buffers();
    let mut replacement_spool = RefSpool::default();
    let replaced = replace_file_v1(
        b"dir/file.bin",
        0o644,
        data.len() as u64,
        &mut replacement_source,
        &mut sink,
        &mut replacement_spool,
        ContentBuffersV1::new(&mut replacement_buffer, &mut replacement_ring),
        &mut ContinueCdcControlV1,
        &ledger,
        &mut counters,
    )
    .unwrap();
    assert_eq!(replaced, created);
    assert!(counters.logical_chunks_reused > 0);
    assert!(counters.physical_objects_reused > 0);
    assert!(counters.has_zero_forbidden_work());
}

#[test]
fn validation_and_reservation_happen_before_source_consumption() {
    let data = b"content";
    let mut source = SliceSource::new(data);
    let mut sink = CapturingSink::default();
    let mut spool = RefSpool::default();
    let (mut source_buffer, mut ring) = buffers();
    let mut counters = OperationCountersV1::default();
    let no_slots = ResourceLedgerV1::new(BASE_LEDGER_BYTES);
    let error = create_file_v1(
        b"file",
        0o644,
        data.len() as u64,
        &mut source,
        &mut sink,
        &mut spool,
        ContentBuffersV1::new(&mut source_buffer, &mut ring),
        &mut ContinueCdcControlV1,
        &no_slots,
        &mut counters,
    )
    .unwrap_err();
    assert_eq!(error, CoreError::ResourceRefused);
    assert_eq!(source.reads, 0);
    assert!(!sink.active);

    let mut oversized_spool = RefSpool {
        reported_resident_bytes: Some(OPERATION_SLOT_BYTES),
        ..RefSpool::default()
    };
    let error = create_file_v1(
        b"file",
        0o644,
        data.len() as u64,
        &mut source,
        &mut sink,
        &mut oversized_spool,
        ContentBuffersV1::new(&mut source_buffer, &mut ring),
        &mut ContinueCdcControlV1,
        &ResourceLedgerV1::new(32 * 1024 * 1024),
        &mut counters,
    )
    .unwrap_err();
    assert_eq!(error, CoreError::ResourceRefused);
    assert_eq!(source.reads, 0);
    assert!(!sink.active);

    let mut oversized_source = SliceSource::new(data);
    oversized_source.resident_memory = OPERATION_SLOT_BYTES;
    let error = create_file_v1(
        b"file",
        0o644,
        data.len() as u64,
        &mut oversized_source,
        &mut sink,
        &mut spool,
        ContentBuffersV1::new(&mut source_buffer, &mut ring),
        &mut ContinueCdcControlV1,
        &ResourceLedgerV1::new(32 * 1024 * 1024),
        &mut counters,
    )
    .unwrap_err();
    assert_eq!(error, CoreError::ResourceRefused);
    assert_eq!(oversized_source.reads, 0);
    assert!(!sink.active);

    let mut source = SliceSource::new(data);
    let mut oversized_sink = CapturingSink {
        resident_memory: OPERATION_SLOT_BYTES,
        ..CapturingSink::default()
    };
    let error = create_file_v1(
        b"file",
        0o644,
        data.len() as u64,
        &mut source,
        &mut oversized_sink,
        &mut spool,
        ContentBuffersV1::new(&mut source_buffer, &mut ring),
        &mut ContinueCdcControlV1,
        &ResourceLedgerV1::new(32 * 1024 * 1024),
        &mut counters,
    )
    .unwrap_err();
    assert_eq!(error, CoreError::ResourceRefused);
    assert_eq!(source.reads, 0);
    assert!(!oversized_sink.active);

    let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
    let error = create_file_v1(
        b"/absolute",
        0o644,
        data.len() as u64,
        &mut source,
        &mut sink,
        &mut spool,
        ContentBuffersV1::new(&mut source_buffer, &mut ring),
        &mut ContinueCdcControlV1,
        &ledger,
        &mut counters,
    )
    .unwrap_err();
    assert_eq!(error, CoreError::Path);
    assert_eq!(source.reads, 0);
}

#[test]
fn truncation_trailing_input_and_sink_refusal_abort_private_state() {
    let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
    for (data, declared, expected) in [
        (&b"short"[..], 6, CoreError::Truncated),
        (&b"trailing"[..], 7, CoreError::TrailingBytes),
    ] {
        let mut source = SliceSource::new(data);
        let mut sink = CapturingSink::default();
        let mut spool = RefSpool::default();
        let (mut source_buffer, mut ring) = buffers();
        let error = create_file_v1(
            b"file",
            0o600,
            declared,
            &mut source,
            &mut sink,
            &mut spool,
            ContentBuffersV1::new(&mut source_buffer, &mut ring),
            &mut ContinueCdcControlV1,
            &ledger,
            &mut OperationCountersV1::default(),
        )
        .unwrap_err();
        assert_eq!(error, expected);
        assert_eq!(sink.completed, None);
        assert_eq!(sink.aborts, 1);
        assert!(spool.aborted);
        assert_eq!(ledger.admitted_slots(), 0);
    }

    let data = support::fastcdc_golden_input(40_000);
    let mut source = SliceSource::new(&data);
    let mut sink = CapturingSink {
        refuse_after: Some(60),
        ..CapturingSink::default()
    };
    let mut spool = RefSpool::default();
    let (mut source_buffer, mut ring) = buffers();
    let error = create_file_v1(
        b"file",
        0o600,
        data.len() as u64,
        &mut source,
        &mut sink,
        &mut spool,
        ContentBuffersV1::new(&mut source_buffer, &mut ring),
        &mut ContinueCdcControlV1,
        &ledger,
        &mut OperationCountersV1::default(),
    )
    .unwrap_err();
    assert_eq!(error, CoreError::SinkRefused);
    assert_eq!(sink.completed, None);
    assert_eq!(sink.aborts, 1);
    assert_eq!(ledger.admitted_slots(), 0);
}

#[test]
fn invalid_source_count_is_rejected_without_slicing_or_partial_visibility() {
    let mut sink = CapturingSink::default();
    let mut spool = RefSpool::default();
    let (mut source_buffer, mut ring) = buffers();
    let mut counters = OperationCountersV1::default();
    assert_eq!(
        create_file_v1(
            b"file.bin",
            0o644,
            1,
            &mut InvalidCountSource,
            &mut sink,
            &mut spool,
            ContentBuffersV1::new(&mut source_buffer, &mut ring),
            &mut ContinueCdcControlV1,
            &ResourceLedgerV1::new(32 * 1024 * 1024),
            &mut counters,
        ),
        Err(CoreError::SourceFailure)
    );
    assert_eq!(counters.bytes_read, 0);
    assert_eq!(counters.bytes_copied, 0);
    assert_eq!(sink.completed, None);
    assert_eq!(sink.aborts, 1);
    assert!(spool.aborted);
}

struct PatternSource {
    remaining: u64,
    position: u64,
    maximum_request: usize,
}

impl ContentSourceV1 for PatternSource {
    fn resident_memory_bound_bytes(&self) -> Result<u64, CoreError> {
        Ok(0)
    }

    fn read(&mut self, destination: &mut [u8]) -> Result<usize, ContentSourceErrorV1> {
        self.maximum_request = self.maximum_request.max(destination.len());
        let amount = usize::try_from(self.remaining.min(destination.len() as u64)).unwrap();
        for (offset, byte) in destination[..amount].iter_mut().enumerate() {
            *byte = (self.position + offset as u64).wrapping_mul(131) as u8;
        }
        self.position += amount as u64;
        self.remaining -= amount as u64;
        Ok(amount)
    }
}

#[derive(Default)]
struct MeasuringSink {
    declared: u64,
    written: u64,
    max_segment: usize,
    max_object: u64,
    object_count: u64,
    completed: bool,
}

impl PreparedObjectSinkV1 for MeasuringSink {
    fn resident_memory_bound_bytes(&self) -> Result<u64, CoreError> {
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
        self.max_object = self.max_object.max(complete_len);
        Ok(())
    }

    fn write_private(&mut self, bytes: &[u8]) -> Result<(), PreparedSinkErrorV1> {
        self.max_segment = self.max_segment.max(bytes.len());
        self.written += bytes.len() as u64;
        Ok(())
    }

    fn finish_object(
        &mut self,
        _expected_id: TypedPhysicalObjectIdV1,
    ) -> Result<ObjectDispositionV1, PreparedSinkErrorV1> {
        assert_eq!(self.written, self.declared);
        self.object_count += 1;
        Ok(ObjectDispositionV1::Created)
    }

    fn finish_closure(&mut self, _result: PreparedFileV1) -> Result<(), PreparedSinkErrorV1> {
        self.completed = true;
        Ok(())
    }

    fn abort_closure(&mut self) {
        self.completed = false;
    }
}

#[test]
fn large_source_uses_fixed_io_windows_and_the_qualified_memory_ledger() {
    assert_eq!(admitted_slots_for_budget(32 * 1024 * 1024), 6);
    assert_eq!(admitted_slots_for_budget(48 * 1024 * 1024), 10);
    assert_eq!(admitted_slots_for_budget(72 * 1024 * 1024), 16);

    let capacity_ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
    let reservations = (0..6)
        .map(|_| capacity_ledger.reserve_operation().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(capacity_ledger.admitted_slots(), 6);
    assert_eq!(capacity_ledger.high_water_bytes(), 32 * 1024 * 1024);
    assert!(matches!(
        capacity_ledger.reserve_operation(),
        Err(CoreError::ResourceRefused)
    ));
    drop(reservations);
    assert_eq!(capacity_ledger.admitted_slots(), 0);

    let len = 64 * 1024 * 1024_u64;
    let mut source = PatternSource {
        remaining: len,
        position: 0,
        maximum_request: 0,
    };
    let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
    let mut sink = MeasuringSink::default();
    let mut spool = RefSpool::default();
    let (mut source_buffer, mut ring) = buffers();
    let mut counters = OperationCountersV1::default();
    let prepared = create_file_v1(
        b"large.bin",
        0o644,
        len,
        &mut source,
        &mut sink,
        &mut spool,
        ContentBuffersV1::new(&mut source_buffer, &mut ring),
        &mut ContinueCdcControlV1,
        &ledger,
        &mut counters,
    )
    .unwrap();
    assert!(sink.completed);
    assert_eq!(source.remaining, 0);
    assert!(source.maximum_request <= MAXIMUM_CHUNK_BYTES);
    assert!(sink.max_segment <= MAXIMUM_CHUNK_BYTES);
    assert_eq!(sink.object_count, u64::from(prepared.chunk_count()) + 1);
    assert_eq!(counters.bytes_read, len);
    assert_eq!(counters.bytes_copied, len);
    assert_eq!(
        counters.memory_high_water,
        BASE_LEDGER_BYTES + OPERATION_SLOT_BYTES
    );
    assert_eq!(
        ledger.high_water_bytes(),
        BASE_LEDGER_BYTES + OPERATION_SLOT_BYTES
    );
    let maximum_refs = len.div_ceil(8_192);
    let expected_planned = BASE_LEDGER_BYTES
        + (MAXIMUM_CHUNK_BYTES as u64 * 2)
        + (IDENTITY_HASHER_BYTES_V1 * 2)
        + maximum_refs * core::mem::size_of::<PreparedChunkRefV1>() as u64;
    assert_eq!(ledger.planned_high_water_bytes(), expected_planned);
    assert_eq!(ledger.admitted_slots(), 0);
    assert_eq!(counters.fallback_attempts, 0);
}
