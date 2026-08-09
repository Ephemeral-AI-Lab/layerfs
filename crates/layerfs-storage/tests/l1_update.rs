mod support;

use layerfs_storage::cdc::{ContinueCdcControlV1, FastCdcV1, MAXIMUM_CHUNK_BYTES};
use layerfs_storage::content::update::{
    update_file_v1, AuthenticatedBaseByteReaderV1, AuthenticatedBaseFileV1,
    BaseChunkEvidenceSourceV1, BaseChunkEvidenceV1, BaseReadErrorV1, UpdateBuffersV1,
    UpdateRangeV1, MAX_UPDATE_RESYNCHRONIZATION_BYTES,
};
use layerfs_storage::content::{
    ChunkReferenceSpoolV1, ContentSourceErrorV1, ContentSourceV1, ObjectDispositionV1,
    PreparedChunkRefV1, PreparedFileV1, PreparedObjectSinkV1, PreparedSinkErrorV1,
};
use layerfs_storage::format::PhysicalObjectKindV1;
use layerfs_storage::identity::{
    derive_logical_chunk_v1, derive_logical_file_v1, derive_physical_chunk_id_v1,
    derive_physical_file_id_v1, LogicalChunkRefV1, LogicalFileIdentityV1, PhysicalFileIdV1,
    IDENTITY_HASHER_BYTES_V1,
};
use layerfs_storage::limits::{OperationCountersV1, ResourceLedgerV1, BASE_LEDGER_BYTES};
use layerfs_storage::object::TypedPhysicalObjectIdV1;
use layerfs_storage::profile::ProfileSpecV1;
use layerfs_storage::CoreError;

#[derive(Default)]
struct Spool {
    refs: Vec<PreparedChunkRefV1>,
    cursor: usize,
    maximum: u64,
    aborted: bool,
}

impl ChunkReferenceSpoolV1 for Spool {
    fn resident_memory_bound_bytes(&self, maximum_refs: u64) -> Result<u64, CoreError> {
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

struct SliceSource<'a> {
    bytes: &'a [u8],
    offset: usize,
    reads: u64,
}

impl<'a> SliceSource<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self {
            bytes,
            offset: 0,
            reads: 0,
        }
    }
}

impl ContentSourceV1 for SliceSource<'_> {
    fn resident_memory_bound_bytes(&self) -> Result<u64, CoreError> {
        Ok(0)
    }

    fn read(&mut self, destination: &mut [u8]) -> Result<usize, ContentSourceErrorV1> {
        self.reads += 1;
        let amount = destination.len().min(self.bytes.len() - self.offset);
        destination[..amount].copy_from_slice(&self.bytes[self.offset..self.offset + amount]);
        self.offset += amount;
        Ok(amount)
    }
}

struct BaseBytes<'a> {
    bytes: &'a [u8],
    bytes_read: u64,
    calls: u64,
    fail_on_call: Option<u64>,
    resident_memory: u64,
}

impl AuthenticatedBaseByteReaderV1 for BaseBytes<'_> {
    fn resident_memory_bound_bytes(&self) -> Result<u64, CoreError> {
        Ok(self.resident_memory)
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
        let source = self.bytes.get(start..end).ok_or(BaseReadErrorV1::Missing)?;
        destination.copy_from_slice(source);
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

#[derive(Clone)]
struct Evidence {
    chunks: Vec<BaseChunkEvidenceV1>,
    cursor: usize,
    disable_anchor_lookup: bool,
    resident_memory: u64,
}

impl BaseChunkEvidenceSourceV1 for Evidence {
    fn resident_memory_bound_bytes(&self) -> Result<u64, CoreError> {
        Ok(self.resident_memory)
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
        Ok(self.chunks.iter().copied().find(|chunk| {
            let end = chunk.end().unwrap();
            (chunk.start() <= offset && offset < end)
                || (include_end && offset == end && end == self.total_len())
        }))
    }

    fn at_start(
        &mut self,
        offset: u64,
    ) -> Result<Option<BaseChunkEvidenceV1>, PreparedSinkErrorV1> {
        if self.disable_anchor_lookup {
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

impl Evidence {
    fn total_len(&self) -> u64 {
        self.chunks
            .last()
            .map(|chunk| chunk.end().unwrap())
            .unwrap_or(0)
    }
}

#[derive(Default)]
struct PrivateSink {
    declared: u64,
    written: u64,
    object_ids: Vec<TypedPhysicalObjectIdV1>,
    completed: Option<PreparedFileV1>,
    aborts: u64,
}

impl PreparedObjectSinkV1 for PrivateSink {
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
        Ok(())
    }

    fn write_private(&mut self, bytes: &[u8]) -> Result<(), PreparedSinkErrorV1> {
        self.written += bytes.len() as u64;
        Ok(())
    }

    fn finish_object(
        &mut self,
        expected_id: TypedPhysicalObjectIdV1,
    ) -> Result<ObjectDispositionV1, PreparedSinkErrorV1> {
        assert_eq!(self.written, self.declared);
        self.object_ids.push(expected_id);
        Ok(ObjectDispositionV1::Created)
    }

    fn finish_closure(&mut self, result: PreparedFileV1) -> Result<(), PreparedSinkErrorV1> {
        self.completed = Some(result);
        Ok(())
    }

    fn abort_closure(&mut self) {
        self.aborts += 1;
        self.object_ids.clear();
        self.completed = None;
    }
}

struct ExpectedFile {
    logical: LogicalFileIdentityV1,
    physical: PhysicalFileIdV1,
    evidence: Evidence,
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

fn expected_file(data: &[u8], mode: u16) -> ExpectedFile {
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
    ExpectedFile {
        logical,
        physical,
        evidence: Evidence {
            chunks,
            cursor: 0,
            disable_anchor_lookup: false,
            resident_memory: 0,
        },
    }
}

struct RunResult {
    prepared: Result<PreparedFileV1, CoreError>,
    counters: OperationCountersV1,
    base_bytes_read: u64,
    base_read_calls: u64,
    inserted_reads: u64,
    output: Spool,
    sink: PrivateSink,
    planned_memory_high_water: u64,
}

fn run_update(
    base_data: &[u8],
    evidence: Evidence,
    authenticated: AuthenticatedBaseFileV1,
    range: UpdateRangeV1,
    inserted_data: &[u8],
    budget: u64,
) -> RunResult {
    run_update_with_base_failure(
        base_data,
        evidence,
        authenticated,
        range,
        inserted_data,
        budget,
        None,
    )
}

#[allow(clippy::too_many_arguments)]
fn run_update_with_base_failure(
    base_data: &[u8],
    evidence: Evidence,
    authenticated: AuthenticatedBaseFileV1,
    range: UpdateRangeV1,
    inserted_data: &[u8],
    budget: u64,
    fail_on_base_call: Option<u64>,
) -> RunResult {
    run_update_with_port_residency(
        base_data,
        evidence,
        authenticated,
        range,
        inserted_data,
        budget,
        fail_on_base_call,
        0,
    )
}

#[allow(clippy::too_many_arguments)]
fn run_update_with_port_residency(
    base_data: &[u8],
    evidence: Evidence,
    authenticated: AuthenticatedBaseFileV1,
    range: UpdateRangeV1,
    inserted_data: &[u8],
    budget: u64,
    fail_on_base_call: Option<u64>,
    base_reader_resident_memory: u64,
) -> RunResult {
    let mut base_reader = BaseBytes {
        bytes: base_data,
        bytes_read: 0,
        calls: 0,
        fail_on_call: fail_on_base_call,
        resident_memory: base_reader_resident_memory,
    };
    let mut inserted = SliceSource::new(inserted_data);
    let mut evidence = evidence;
    let mut sink = PrivateSink::default();
    let mut output = Spool::default();
    let mut source_buffer = [0_u8; MAXIMUM_CHUNK_BYTES];
    let mut ring = [0_u8; MAXIMUM_CHUNK_BYTES];
    let ledger = ResourceLedgerV1::new(budget);
    let mut counters = OperationCountersV1::default();
    let prepared = update_file_v1(
        b"file.bin",
        0o644,
        authenticated,
        range,
        inserted_data.len() as u64,
        &mut inserted,
        &mut base_reader,
        &mut evidence,
        &mut sink,
        &mut output,
        UpdateBuffersV1::new(&mut source_buffer, &mut ring),
        &mut ContinueCdcControlV1,
        &ledger,
        &mut counters,
    );
    let planned_memory_high_water = ledger.planned_high_water_bytes();
    assert_eq!(ledger.admitted_slots(), 0);
    RunResult {
        prepared,
        counters,
        base_bytes_read: base_reader.bytes_read,
        base_read_calls: base_reader.calls,
        inserted_reads: inserted.reads,
        output,
        sink,
        planned_memory_high_water,
    }
}

fn edited(base: &[u8], start: usize, end: usize, inserted: &[u8]) -> Vec<u8> {
    let mut result = Vec::with_capacity(base.len() - (end - start) + inserted.len());
    result.extend_from_slice(&base[..start]);
    result.extend_from_slice(inserted);
    result.extend_from_slice(&base[end..]);
    result
}

#[test]
fn insertion_deletion_replacement_and_edge_ranges_match_full_canonical_results() {
    let base_data = support::fastcdc_golden_input(220_000);
    let base = expected_file(&base_data, 0o644);
    let authenticated = AuthenticatedBaseFileV1::new(
        base.logical,
        base.physical,
        0o644,
        u32::try_from(base.evidence.chunks.len()).unwrap(),
    );
    let first_chunk_end = usize::try_from(base.evidence.chunks[0].end().unwrap()).unwrap();
    let cases = [
        (50_000, 50_000, b"inserted bytes".as_slice()),
        (60_000, 75_000, b"".as_slice()),
        (31_000, 31_018, b"same length bytes!".as_slice()),
        (0, first_chunk_end, b"".as_slice()),
        (210_000, 220_000, b"tail replacement".as_slice()),
    ];
    for (start, end, insertion) in cases {
        let expected_data = edited(&base_data, start, end, insertion);
        let expected = expected_file(&expected_data, 0o644);
        let result = run_update(
            &base_data,
            base.evidence.clone(),
            authenticated,
            UpdateRangeV1::new(start as u64, end as u64, base_data.len() as u64).unwrap(),
            insertion,
            32 * 1024 * 1024,
        );
        let prepared = result
            .prepared
            .unwrap_or_else(|error| panic!("range {start}..{end}: {error:?}"));
        assert_eq!(
            prepared.logical_file(),
            expected.logical,
            "range {start}..{end}"
        );
        assert_eq!(
            prepared.physical_file(),
            expected.physical,
            "range {start}..{end}"
        );
        assert_eq!(result.sink.completed, Some(prepared));
        assert_eq!(result.output.refs.len(), prepared.chunk_count() as usize);
        assert!(
            result.base_bytes_read
                <= MAXIMUM_CHUNK_BYTES as u64 + MAX_UPDATE_RESYNCHRONIZATION_BYTES
        );
        assert!(
            result.counters.update_resynchronization_bytes <= MAX_UPDATE_RESYNCHRONIZATION_BYTES
        );
        assert_eq!(result.counters.fallback_attempts, 0);
        assert_eq!(result.counters.retries_or_redispatches, 0);
        assert_eq!(result.counters.publication_dispatches, 0);
        let maximum_refs = (expected_data.len() as u64).div_ceil(8_192);
        assert_eq!(
            result.planned_memory_high_water,
            BASE_LEDGER_BYTES
                + (MAXIMUM_CHUNK_BYTES as u64 * 2)
                + (IDENTITY_HASHER_BYTES_V1 * 2)
                + maximum_refs * core::mem::size_of::<PreparedChunkRefV1>() as u64
        );
    }
}

#[test]
fn middle_update_reuses_authenticated_prefix_and_suffix_identities() {
    let base_data = support::fastcdc_golden_input(300_000);
    let base = expected_file(&base_data, 0o644);
    let authenticated = AuthenticatedBaseFileV1::new(
        base.logical,
        base.physical,
        0o644,
        base.evidence.chunks.len() as u32,
    );
    let result = run_update(
        &base_data,
        base.evidence.clone(),
        authenticated,
        UpdateRangeV1::new(120_000, 120_010, base_data.len() as u64).unwrap(),
        b"changed",
        32 * 1024 * 1024,
    );
    let prepared = result.prepared.unwrap();
    assert!(result.counters.logical_chunks_reused >= 2);
    assert!(result.counters.bytes_structurally_reused > 0);
    assert!(result.counters.anchor_attempts > 0);
    assert!(result.sink.object_ids.len() < prepared.chunk_count() as usize + 1);
    assert_eq!(result.counters.fallback_attempts, 0);
}

#[test]
fn rejoin_requires_a_complete_exact_base_byte_comparison() {
    let base_data = support::fastcdc_golden_input(300_000);
    let base = expected_file(&base_data, 0o644);
    let authenticated = AuthenticatedBaseFileV1::new(
        base.logical,
        base.physical,
        0o644,
        base.evidence.chunks.len() as u32,
    );
    let range = UpdateRangeV1::new(120_000, 120_010, base_data.len() as u64).unwrap();
    let successful = run_update(
        &base_data,
        base.evidence.clone(),
        authenticated,
        range,
        b"changed",
        32 * 1024 * 1024,
    );
    assert!(successful.prepared.is_ok());
    assert!(successful.base_read_calls >= 2);

    let failed_proof = run_update_with_base_failure(
        &base_data,
        base.evidence,
        authenticated,
        range,
        b"changed",
        32 * 1024 * 1024,
        Some(successful.base_read_calls),
    );
    assert_eq!(failed_proof.prepared, Err(CoreError::RangeResyncFailed));
    assert_eq!(failed_proof.sink.completed, None);
    assert_eq!(failed_proof.sink.aborts, 1);
    assert!(failed_proof.output.aborted);
    assert_eq!(failed_proof.counters.fallback_attempts, 0);
}

#[test]
fn missing_or_mismatched_evidence_and_no_anchor_fail_closed() {
    let base_data = support::fastcdc_golden_input(300_000);
    let base = expected_file(&base_data, 0o644);
    let authenticated = AuthenticatedBaseFileV1::new(
        base.logical,
        base.physical,
        0o644,
        base.evidence.chunks.len() as u32,
    );
    let range = UpdateRangeV1::new(1_000, 1_010, base_data.len() as u64).unwrap();

    let mut missing = base.evidence.clone();
    missing.chunks.pop();
    let result = run_update(
        &base_data,
        missing,
        authenticated,
        range,
        b"x",
        32 * 1024 * 1024,
    );
    assert_eq!(result.prepared, Err(CoreError::RangeResyncFailed));
    assert_eq!(result.base_bytes_read, 0);
    assert_eq!(result.inserted_reads, 0);
    assert_eq!(result.counters.update_failures, 1);

    let other = expected_file(b"different", 0o644);
    let mismatched = AuthenticatedBaseFileV1::new(
        other.logical,
        other.physical,
        0o644,
        base.evidence.chunks.len() as u32,
    );
    let result = run_update(
        &base_data,
        base.evidence.clone(),
        mismatched,
        range,
        b"x",
        32 * 1024 * 1024,
    );
    assert_eq!(result.prepared, Err(CoreError::RangeResyncFailed));
    assert_eq!(result.base_bytes_read, 0);
    assert_eq!(result.inserted_reads, 0);

    let physical_mismatch = AuthenticatedBaseFileV1::new(
        base.logical,
        other.physical,
        0o644,
        base.evidence.chunks.len() as u32,
    );
    let result = run_update(
        &base_data,
        base.evidence.clone(),
        physical_mismatch,
        range,
        b"x",
        32 * 1024 * 1024,
    );
    assert_eq!(result.prepared, Err(CoreError::RangeResyncFailed));
    assert_eq!(result.base_bytes_read, 0);
    assert_eq!(result.inserted_reads, 0);

    let mut no_anchor = base.evidence.clone();
    no_anchor.disable_anchor_lookup = true;
    let result = run_update(
        &base_data,
        no_anchor,
        authenticated,
        range,
        b"x",
        32 * 1024 * 1024,
    );
    assert_eq!(result.prepared, Err(CoreError::RangeResyncFailed));
    assert!(result.counters.update_resynchronization_bytes > 0);
    assert!(result.counters.update_resynchronization_bytes <= MAX_UPDATE_RESYNCHRONIZATION_BYTES);
    assert!(result.base_bytes_read < base_data.len() as u64);
    assert_eq!(result.counters.fallback_attempts, 0);
    assert_eq!(result.sink.completed, None);
    assert_eq!(result.sink.aborts, 1);
    assert!(result.output.aborted);
}

#[test]
fn invalid_range_and_resource_refusal_precede_all_reads() {
    assert_eq!(
        UpdateRangeV1::new(11, 10, 20),
        Err(CoreError::RangeResyncFailed)
    );
    assert_eq!(
        UpdateRangeV1::new(0, 21, 20),
        Err(CoreError::RangeResyncFailed)
    );

    let base_data = support::fastcdc_golden_input(40_000);
    let base = expected_file(&base_data, 0o644);
    let authenticated = AuthenticatedBaseFileV1::new(
        base.logical,
        base.physical,
        0o644,
        base.evidence.chunks.len() as u32,
    );
    let result = run_update(
        &base_data,
        base.evidence,
        authenticated,
        UpdateRangeV1::new(10, 20, base_data.len() as u64).unwrap(),
        b"x",
        BASE_LEDGER_BYTES,
    );
    assert_eq!(result.prepared, Err(CoreError::ResourceRefused));
    assert_eq!(result.base_bytes_read, 0);
    assert_eq!(result.inserted_reads, 0);
    assert_eq!(result.counters.fallback_attempts, 0);

    let base = expected_file(&base_data, 0o644);
    let authenticated = AuthenticatedBaseFileV1::new(
        base.logical,
        base.physical,
        0o644,
        base.evidence.chunks.len() as u32,
    );
    let mut oversized_evidence = base.evidence.clone();
    oversized_evidence.resident_memory = 4 * 1024 * 1024;
    let result = run_update(
        &base_data,
        oversized_evidence,
        authenticated,
        UpdateRangeV1::new(10, 20, base_data.len() as u64).unwrap(),
        b"x",
        32 * 1024 * 1024,
    );
    assert_eq!(result.prepared, Err(CoreError::RangeResyncFailed));
    assert_eq!(result.base_bytes_read, 0);
    assert_eq!(result.inserted_reads, 0);

    let result = run_update_with_port_residency(
        &base_data,
        base.evidence,
        authenticated,
        UpdateRangeV1::new(10, 20, base_data.len() as u64).unwrap(),
        b"x",
        32 * 1024 * 1024,
        None,
        4 * 1024 * 1024,
    );
    assert_eq!(result.prepared, Err(CoreError::RangeResyncFailed));
    assert_eq!(result.base_bytes_read, 0);
    assert_eq!(result.inserted_reads, 0);
}
