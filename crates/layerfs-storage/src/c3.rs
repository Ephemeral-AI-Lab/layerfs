//! Closed C3 operation and portable fixed-record file spools.
//!
//! This module exists only behind the C3 polymorphism feature. It deliberately
//! exposes no publication authority: the terminal result is a synchronous,
//! consumed storage handoff. One ledger reservation is acquired before the
//! source supplier is invoked and is borrowed by every lower layer.

use core::cmp::Ordering;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use crate::cas::AdmissionBuffersV1;
use crate::cdc::{algorithms::C3CdcAlgorithmV1, CdcControlV1, MAXIMUM_CHUNK_BYTES};
use crate::content::{
    create_file_c3_borrowed_v1, ChunkReferenceSpoolV1, ContentBuffersV1, ContentSourceV1,
    ObjectDispositionV1, PreparedChunkRefV1, PreparedFileV1, PreparedObjectSinkV1,
    PreparedSinkErrorV1,
};
use crate::format::{
    validate_chunk_refs_per_file, validate_physical_object_len, PhysicalObjectKindV1,
    ValidatedComponent,
};
use crate::fscas::{
    FsCasControlV1, FsCasErrorV1, FsCasV1, FsPackAdmissionOutcomeV1, FsPrivatePackV1,
};
use crate::identity::{
    derive_file_node_v1, derive_physical_version_record_id_v1, derive_version_v1, FramedHasherV1,
    LogicalChunkIdV1, ObjectChecksumV1, PackIdV1, PhysicalChunkIdV1, PhysicalTreeIdV1,
    PhysicalVersionRecordIdV1, COMPARISON_WINDOW_BYTES, IDENTITY_HASHER_BYTES_V1,
    TAG_OBJECT_CHECKSUM, TAG_PACK,
};
use crate::limits::{
    CounterFieldV1, MemoryComponentV1, OperationCountersV1, OperationMemoryPlanV1, ResourceLedgerV1,
};
use crate::object::{TypedPhysicalObjectIdV1, VERSION_RECORD_PAYLOAD_BYTES};
use crate::pack::{
    encode_header, encode_index_entry, encode_trailer_prefix, hash_port_range, record_padding,
    PackIndexEntryV1, PackIndexSpoolV1, PackPortErrorV1, PackReadPortV1, PrivatePackPortV1,
    SealedPackV1, MAX_PACK_BYTES, MAX_PACK_RECORDS, PACK_INDEX_ENTRY_BYTES, PACK_TRAILER_BYTES,
};
use crate::profile::{ChunkerSpecV1, DigestSpecV1};
use crate::tree::{
    build_canonical_directory_borrowed_v1, CanonicalTreeChildV1, CanonicalTreeEntryV1,
    DirectoryBuildModeV1, DirectoryLogicalIdentityV1, PreparedTreeSinkV1, TreeObjectDispositionV1,
    TreePageSummaryV1, TreeSinkErrorV1, MAX_TREE_OBJECT_BYTES,
};
use crate::{CoreError, CoreResult};

const CHUNK_REFERENCE_RECORD_BYTES: u64 = 68;
const VERSION_OBJECT_BYTES: usize = 52 + VERSION_RECORD_PAYLOAD_BYTES as usize;
const DEFAULT_METADATA_RESERVATION_BYTES: u64 = 1_048_576;
const CLOSURE_TRAVERSAL_SUMMARY_BYTES: u64 = 1_028 * 512;

pub trait C3SourceSupplierV1 {
    type Source: ContentSourceV1;

    /// Side-effect-free bound available before the operation slot is held.
    fn resident_memory_bound_bytes(&self) -> CoreResult<u64>;
    fn supply(self) -> CoreResult<Self::Source>;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum C3OperationErrorV1 {
    Core(CoreError),
    FsCas(FsCasErrorV1),
}

impl From<CoreError> for C3OperationErrorV1 {
    fn from(error: CoreError) -> Self {
        Self::Core(error)
    }
}

impl From<FsCasErrorV1> for C3OperationErrorV1 {
    fn from(error: FsCasErrorV1) -> Self {
        Self::FsCas(error)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct C3HandoffV1 {
    algorithm: C3CdcAlgorithmV1,
    version_record: PhysicalVersionRecordIdV1,
    root_tree: PhysicalTreeIdV1,
    pack: SealedPackV1,
    pack_outcome: FsPackAdmissionOutcomeV1,
    object_count: u64,
    reference_spool_bytes: Option<u64>,
    index_spool_bytes: Option<u64>,
}

impl C3HandoffV1 {
    pub const fn algorithm(self) -> C3CdcAlgorithmV1 {
        self.algorithm
    }

    pub const fn version_record(self) -> PhysicalVersionRecordIdV1 {
        self.version_record
    }

    pub const fn root_tree(self) -> PhysicalTreeIdV1 {
        self.root_tree
    }

    pub const fn pack(self) -> SealedPackV1 {
        self.pack
    }

    pub const fn pack_outcome(self) -> FsPackAdmissionOutcomeV1 {
        self.pack_outcome
    }

    pub const fn object_count(self) -> u64 {
        self.object_count
    }

    pub const fn reference_spool_bytes(self) -> Option<u64> {
        self.reference_spool_bytes
    }

    pub const fn index_spool_bytes(self) -> Option<u64> {
        self.index_spool_bytes
    }
}

pub struct C3OperationBuffersV1<'a> {
    pub source: &'a mut [u8; MAXIMUM_CHUNK_BYTES],
    pub cdc_ring: &'a mut [u8; MAXIMUM_CHUNK_BYTES],
    pub incoming_comparison: &'a mut [u8; COMPARISON_WINDOW_BYTES],
    pub occupied_comparison: &'a mut [u8; COMPARISON_WINDOW_BYTES],
    pub tree_object: &'a mut [u8; MAX_TREE_OBJECT_BYTES],
    pub tree_pages: &'a mut [Option<TreePageSummaryV1>],
    pub traversal_state: &'a mut [u8],
}

#[allow(clippy::too_many_arguments)]
pub fn run_c3_create_v1<S, R, M, C>(
    cas: &FsCasV1,
    algorithm: C3CdcAlgorithmV1,
    name: &[u8],
    mode: u16,
    declared_len: u64,
    supplier: S,
    references: &mut R,
    metadata: &mut M,
    buffers: C3OperationBuffersV1<'_>,
    control: &mut C,
    ledger: &ResourceLedgerV1,
    counters: &mut OperationCountersV1,
) -> Result<C3HandoffV1, C3OperationErrorV1>
where
    S: C3SourceSupplierV1,
    R: ChunkReferenceSpoolV1 + ?Sized,
    M: PackIndexSpoolV1 + ?Sized,
    C: CdcControlV1 + FsCasControlV1 + ?Sized,
{
    let component = ValidatedComponent::new(name)?;
    let maximum_refs = declared_len
        .checked_add(8_191)
        .ok_or(CoreError::IntegerOverflow)?
        / 8_192;
    validate_chunk_refs_per_file(maximum_refs)?;
    let maximum_records = maximum_refs
        .checked_add(4)
        .ok_or(CoreError::IntegerOverflow)?;
    if maximum_records > MAX_PACK_RECORDS {
        return Err(CoreError::CountCap.into());
    }
    let maximum_records_u32 =
        u32::try_from(maximum_records).map_err(|_| CoreError::IntegerOverflow)?;

    // Creating the handle allocates no carrier bytes. Its exact declaration,
    // the supplier declaration, and both spool declarations are all known
    // before the source supplier can be invoked.
    let mut private_pack = cas.begin_private_pack()?;
    let supplier_resident = supplier.resident_memory_bound_bytes()?;
    let reference_resident = references.resident_memory_bound_bytes(maximum_refs)?;
    let index_resident = metadata.resident_memory_bound_bytes(maximum_records_u32)?;
    let private_pack_resident = private_pack.resident_memory_bound_bytes()?;
    let port_resident = supplier_resident
        .checked_add(reference_resident)
        .and_then(|bytes| bytes.checked_add(index_resident))
        .and_then(|bytes| bytes.checked_add(private_pack_resident))
        .ok_or(CoreError::IntegerOverflow)?;
    let metadata_reservation = port_resident.max(DEFAULT_METADATA_RESERVATION_BYTES);
    let plan = OperationMemoryPlanV1::empty()
        .charge(MemoryComponentV1::SourceWindow, buffers.source.len() as u64)?
        .charge(MemoryComponentV1::CdcRing, buffers.cdc_ring.len() as u64)?
        .charge(
            MemoryComponentV1::ComparisonWindow,
            (2 * COMPARISON_WINDOW_BYTES) as u64,
        )?
        .charge(
            MemoryComponentV1::ObjectScratch,
            buffers.tree_object.len() as u64,
        )?
        .charge(
            MemoryComponentV1::PageSummaries,
            CLOSURE_TRAVERSAL_SUMMARY_BYTES.max(core::mem::size_of_val(buffers.tree_pages) as u64),
        )?
        .charge(
            MemoryComponentV1::TraversalState,
            buffers.traversal_state.len() as u64,
        )?
        .charge(MemoryComponentV1::MetadataWindow, metadata_reservation)?
        .charge(
            MemoryComponentV1::HashState,
            IDENTITY_HASHER_BYTES_V1
                .checked_mul(2)
                .ok_or(CoreError::IntegerOverflow)?,
        )?;
    let reservation = ledger.reserve_operation_with_plan(plan)?;
    counters.memory_high_water = counters.memory_high_water.max(ledger.high_water_bytes());

    let mut source = match supplier.supply() {
        Ok(source) => source,
        Err(error) => {
            references.abort();
            metadata.abort();
            return Err(error.into());
        }
    };
    if source.resident_memory_bound_bytes()? > supplier_resident {
        references.abort();
        metadata.abort();
        return Err(CoreError::ResourceRefused.into());
    }

    let preparation = (|| -> Result<_, C3OperationErrorV1> {
        let mut sink = DirectPackSinkV1::new(
            &mut private_pack,
            metadata,
            buffers.incoming_comparison,
            buffers.occupied_comparison,
            buffers.traversal_state,
            maximum_records_u32,
        );
        let file = create_file_c3_borrowed_v1(
            name,
            mode,
            declared_len,
            &mut source,
            &mut sink,
            references,
            ContentBuffersV1::new(buffers.source, buffers.cdc_ring),
            control,
            &reservation,
            algorithm,
            counters,
        )?;
        let file_node = derive_file_node_v1(mode, file.logical_file())?;
        let entry = CanonicalTreeEntryV1::new(
            component,
            CanonicalTreeChildV1::File {
                logical: file_node,
                physical: file.physical_file(),
            },
        );
        let tree = build_canonical_directory_borrowed_v1(
            DirectoryBuildModeV1::ImplicitRoot,
            &[entry],
            &mut sink,
            &reservation,
            counters,
            buffers.tree_object,
            buffers.tree_pages,
        )?;
        let DirectoryLogicalIdentityV1::ImplicitRoot(logical_root) = tree.logical() else {
            return Err(CoreError::TypeDomain.into());
        };
        let version = sink.write_version_v1(
            derive_version_v1(logical_root),
            tree.physical(),
            declared_len,
            file,
            tree.tree_object_count(),
            counters,
        )?;
        let sealed = sink.finalize_v1(counters)?;
        Ok((version, tree.physical(), sealed))
    })();
    let (version, root_tree, prepared_seal) = match preparation {
        Ok(prepared) => prepared,
        Err(error) => {
            private_pack.abort_private();
            references.abort();
            metadata.abort();
            return Err(error);
        }
    };

    let operation = (|| -> Result<_, C3OperationErrorV1> {
        let admission = cas.admit_pack_borrowed_controlled_v1(
            &mut private_pack,
            metadata,
            ledger,
            &reservation,
            counters,
            buffers.incoming_comparison,
            control,
        )?;
        if admission.sealed() != prepared_seal {
            return Err(CoreError::PackInvalid.into());
        }
        let mut closure = cas.open_admitted_pack_closure_v1(admission.sealed())?;
        let mut closure_operation = cas.begin_closure_operation()?;
        let closure_result = cas.admit_complete_closure_borrowed_v1(
            &mut closure_operation,
            &mut closure,
            TypedPhysicalObjectIdV1::VersionRecord(version),
            &reservation,
            counters,
            AdmissionBuffersV1::new(
                buffers.incoming_comparison,
                buffers.occupied_comparison,
                buffers.source,
                buffers.cdc_ring,
                buffers.traversal_state,
            ),
            algorithm,
        );
        let (admitted, mut capability) = match closure_result {
            Ok(value) => value,
            Err(error) => {
                admission.record_later_unreachable_residue(counters)?;
                return Err(error.into());
            }
        };
        cas.consume_validated_closure_for_handoff(&mut closure_operation, &mut capability)?;
        Ok(C3HandoffV1 {
            algorithm,
            version_record: version,
            root_tree,
            pack: admission.sealed(),
            pack_outcome: admission.outcome(),
            object_count: admitted.object_count(),
            reference_spool_bytes: references.storage_bytes_observation()?,
            index_spool_bytes: metadata.storage_bytes_observation()?,
        })
    })();

    if operation.is_err() {
        private_pack.abort_private();
    }
    references.abort();
    metadata.abort();
    operation
}

struct CurrentObjectV1 {
    kind: PhysicalObjectKindV1,
    record_offset: u64,
    complete_len: u64,
    written: u64,
    checksum: FramedHasherV1,
}

struct DirectPackSinkV1<'a, M: ?Sized> {
    pack: &'a mut FsPrivatePackV1,
    metadata: &'a mut M,
    left: &'a mut [u8; COMPARISON_WINDOW_BYTES],
    right: &'a mut [u8; COMPARISON_WINDOW_BYTES],
    seen_filter: &'a mut [u8],
    maximum_records: u32,
    record_count: u32,
    kind_counts: [u32; 5],
    physical_chunk_bytes: u64,
    current: Option<CurrentObjectV1>,
    active: bool,
    direct_write_bytes: u64,
    direct_read_bytes: u64,
    direct_read_calls: u64,
}

impl<'a, M: PackIndexSpoolV1 + ?Sized> DirectPackSinkV1<'a, M> {
    fn new(
        pack: &'a mut FsPrivatePackV1,
        metadata: &'a mut M,
        left: &'a mut [u8; COMPARISON_WINDOW_BYTES],
        right: &'a mut [u8; COMPARISON_WINDOW_BYTES],
        seen_filter: &'a mut [u8],
        maximum_records: u32,
    ) -> Self {
        Self {
            pack,
            metadata,
            left,
            right,
            seen_filter,
            maximum_records,
            record_count: 0,
            kind_counts: [0; 5],
            physical_chunk_bytes: 0,
            current: None,
            active: false,
            direct_write_bytes: 0,
            direct_read_bytes: 0,
            direct_read_calls: 0,
        }
    }

    fn begin_object_inner(
        &mut self,
        kind: PhysicalObjectKindV1,
        complete_len: u64,
    ) -> CoreResult<()> {
        if !self.active || self.current.is_some() {
            return Err(CoreError::SinkRefused);
        }
        validate_physical_object_len(complete_len)?;
        let record_offset = self.pack.len().map_err(map_pack_read)?;
        let object_len = u32::try_from(complete_len).map_err(|_| CoreError::IntegerOverflow)?;
        self.append_pack(&object_len.to_be_bytes())?;
        self.current = Some(CurrentObjectV1 {
            kind,
            record_offset,
            complete_len,
            written: 0,
            checksum: FramedHasherV1::new(TAG_OBJECT_CHECKSUM, complete_len),
        });
        Ok(())
    }

    fn write_inner(&mut self, bytes: &[u8]) -> CoreResult<()> {
        let current = self.current.as_mut().ok_or(CoreError::SinkRefused)?;
        let amount = u64::try_from(bytes.len()).map_err(|_| CoreError::IntegerOverflow)?;
        let next = current
            .written
            .checked_add(amount)
            .ok_or(CoreError::IntegerOverflow)?;
        if next > current.complete_len {
            return Err(CoreError::TrailingBytes);
        }
        current.checksum.write(bytes)?;
        self.pack.append(bytes).map_err(map_pack_write)?;
        self.direct_write_bytes = self
            .direct_write_bytes
            .checked_add(amount)
            .ok_or(CoreError::IntegerOverflow)?;
        current.written = next;
        Ok(())
    }

    fn finish_object_inner(
        &mut self,
        expected_id: TypedPhysicalObjectIdV1,
    ) -> CoreResult<ObjectDispositionV1> {
        let current = self.current.take().ok_or(CoreError::SinkRefused)?;
        if current.written != current.complete_len || current.kind != expected_id.kind() {
            return Err(CoreError::IdMismatch);
        }
        let mut incumbent = None;
        if self.seen_filter_may_contain(expected_id) {
            self.metadata.rewind().map_err(map_spool)?;
            while let Some(entry) = self.metadata.next().map_err(map_spool)? {
                if entry.id() == expected_id {
                    incumbent = Some(entry);
                    break;
                }
            }
        }
        if let Some(entry) = incumbent {
            if u64::from(entry.object_len()) != current.complete_len
                || !self.compare_objects(
                    entry.absolute_offset() + 4,
                    current.record_offset + 4,
                    current.complete_len,
                )?
            {
                return Err(CoreError::IdMismatch);
            }
            self.pack
                .truncate_direct_v1(current.record_offset)
                .map_err(map_pack_write)?;
            return Ok(ObjectDispositionV1::Reused);
        }

        let checksum = ObjectChecksumV1::from_digest(current.checksum.finish()?);
        let padding = record_padding(current.complete_len)?;
        if padding != 0 {
            self.append_pack(&[0_u8; 7][..usize::from(padding)])?;
        }
        if self.record_count >= self.maximum_records {
            return Err(CoreError::CountCap);
        }
        self.metadata
            .push(PackIndexEntryV1::from_validated_parts(
                expected_id,
                current.record_offset,
                u32::try_from(current.complete_len).map_err(|_| CoreError::IntegerOverflow)?,
                checksum,
            ))
            .map_err(map_spool)?;
        self.note_seen(expected_id);
        self.record_count = self
            .record_count
            .checked_add(1)
            .ok_or(CoreError::IntegerOverflow)?;
        let kind_index = kind_index(current.kind);
        self.kind_counts[kind_index] = self.kind_counts[kind_index]
            .checked_add(1)
            .ok_or(CoreError::IntegerOverflow)?;
        if current.kind == PhysicalObjectKindV1::Chunk {
            self.physical_chunk_bytes = self
                .physical_chunk_bytes
                .checked_add(
                    current
                        .complete_len
                        .checked_sub(52)
                        .ok_or(CoreError::IntegerOverflow)?,
                )
                .ok_or(CoreError::IntegerOverflow)?;
        }
        Ok(ObjectDispositionV1::Created)
    }

    fn seen_filter_indexes(&self, id: TypedPhysicalObjectIdV1) -> Option<(usize, usize)> {
        let bit_count = self.seen_filter.len().checked_mul(8)?;
        if bit_count == 0 {
            return None;
        }
        let bytes = id.as_bytes();
        let kind = kind_index(id.kind());
        let first = usize::from(u16::from_be_bytes([bytes[0], bytes[1]])) ^ kind;
        let second = usize::from(u16::from_be_bytes([bytes[30], bytes[31]])).wrapping_mul(0x9e37)
            ^ kind.wrapping_mul(0x85eb);
        Some((first % bit_count, second % bit_count))
    }

    fn seen_filter_may_contain(&self, id: TypedPhysicalObjectIdV1) -> bool {
        let Some((first, second)) = self.seen_filter_indexes(id) else {
            return true;
        };
        self.seen_filter[first / 8] & (1 << (first % 8)) != 0
            && self.seen_filter[second / 8] & (1 << (second % 8)) != 0
    }

    fn note_seen(&mut self, id: TypedPhysicalObjectIdV1) {
        if let Some((first, second)) = self.seen_filter_indexes(id) {
            self.seen_filter[first / 8] |= 1 << (first % 8);
            self.seen_filter[second / 8] |= 1 << (second % 8);
        }
    }

    fn compare_objects(&mut self, left: u64, right: u64, len: u64) -> CoreResult<bool> {
        let mut offset = 0_u64;
        while offset < len {
            let take = usize::try_from((len - offset).min(COMPARISON_WINDOW_BYTES as u64))
                .map_err(|_| CoreError::IntegerOverflow)?;
            self.pack
                .read_exact_at(left + offset, &mut self.left[..take])
                .map_err(map_pack_read)?;
            self.pack
                .read_exact_at(right + offset, &mut self.right[..take])
                .map_err(map_pack_read)?;
            self.direct_read_bytes = self
                .direct_read_bytes
                .checked_add((2 * take) as u64)
                .ok_or(CoreError::IntegerOverflow)?;
            self.direct_read_calls = self
                .direct_read_calls
                .checked_add(2)
                .ok_or(CoreError::IntegerOverflow)?;
            if self.left[..take] != self.right[..take] {
                return Ok(false);
            }
            offset = offset
                .checked_add(take as u64)
                .ok_or(CoreError::IntegerOverflow)?;
        }
        Ok(true)
    }

    fn append_pack(&mut self, bytes: &[u8]) -> CoreResult<()> {
        self.pack.append(bytes).map_err(map_pack_write)?;
        self.direct_write_bytes = self
            .direct_write_bytes
            .checked_add(bytes.len() as u64)
            .ok_or(CoreError::IntegerOverflow)?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn write_version_v1(
        &mut self,
        version_id: crate::identity::VersionIdV1,
        root_tree: PhysicalTreeIdV1,
        canonical_len: u64,
        file: PreparedFileV1,
        tree_count: u32,
        counters: &mut OperationCountersV1,
    ) -> CoreResult<PhysicalVersionRecordIdV1> {
        let total_object_count = self
            .record_count
            .checked_add(1)
            .ok_or(CoreError::IntegerOverflow)?;
        let mut object = [0_u8; VERSION_OBJECT_BYTES];
        object[..52].copy_from_slice(&crate::content::object_header(
            PhysicalObjectKindV1::VersionRecord,
            VERSION_RECORD_PAYLOAD_BYTES,
        ));
        let payload = &mut object[52..];
        payload[0..32].copy_from_slice(version_id.as_bytes());
        payload[32..64].copy_from_slice(ChunkerSpecV1::frozen().id().as_bytes());
        payload[64..96].copy_from_slice(DigestSpecV1::frozen().id().as_bytes());
        payload[96..128].copy_from_slice(root_tree.as_bytes());
        payload[128..136].copy_from_slice(&canonical_len.to_be_bytes());
        payload[136..144].copy_from_slice(&canonical_len.to_be_bytes());
        payload[144..148].copy_from_slice(&1_u32.to_be_bytes());
        payload[148..152].copy_from_slice(&tree_count.to_be_bytes());
        payload[152..156].copy_from_slice(&1_u32.to_be_bytes());
        payload[156..160].copy_from_slice(&0_u32.to_be_bytes());
        payload[160..164].copy_from_slice(
            &self.kind_counts[kind_index(PhysicalObjectKindV1::Chunk)].to_be_bytes(),
        );
        payload[164..168].copy_from_slice(&u32::from(canonical_len != 0).to_be_bytes());
        payload[168..172].copy_from_slice(&file.chunk_count().to_be_bytes());
        payload[172..176].copy_from_slice(&total_object_count.to_be_bytes());
        payload[176..184].copy_from_slice(&self.physical_chunk_bytes.to_be_bytes());
        let id = derive_physical_version_record_id_v1(&object)?;
        self.begin_object_inner(PhysicalObjectKindV1::VersionRecord, object.len() as u64)?;
        self.write_inner(&object)?;
        let disposition = self.finish_object_inner(TypedPhysicalObjectIdV1::VersionRecord(id))?;
        counters.add(CounterFieldV1::BytesWritten, object.len() as u64)?;
        counters.add(CounterFieldV1::PhysicalHashBytes, object.len() as u64)?;
        counters.add(CounterFieldV1::PhysicalHashUpdateCalls, 1)?;
        counters.add(
            match disposition {
                ObjectDispositionV1::Created => CounterFieldV1::PhysicalObjectsCreated,
                ObjectDispositionV1::Reused => CounterFieldV1::PhysicalObjectsReused,
            },
            1,
        )?;
        Ok(id)
    }

    fn finalize_v1(&mut self, counters: &mut OperationCountersV1) -> CoreResult<SealedPackV1> {
        if !self.active || self.current.is_some() || self.record_count == 0 {
            return Err(CoreError::SinkRefused);
        }
        let index_offset = self.pack.len().map_err(map_pack_read)?;
        let index_len = u64::from(self.record_count)
            .checked_mul(PACK_INDEX_ENTRY_BYTES)
            .ok_or(CoreError::IntegerOverflow)?;
        let pack_len = index_offset
            .checked_add(index_len)
            .and_then(|bytes| bytes.checked_add(PACK_TRAILER_BYTES))
            .ok_or(CoreError::IntegerOverflow)?;
        if pack_len > MAX_PACK_BYTES {
            return Err(CoreError::ResourceRefused);
        }
        self.pack
            .patch_direct_v1(0, &encode_header(self.record_count, index_offset))
            .map_err(map_pack_write)?;
        self.direct_write_bytes = self
            .direct_write_bytes
            .checked_add(64)
            .ok_or(CoreError::IntegerOverflow)?;
        self.metadata.sort_by_key().map_err(map_spool)?;
        self.metadata.rewind().map_err(map_spool)?;
        let mut emitted = 0_u32;
        let mut previous = None;
        while let Some(entry) = self.metadata.next().map_err(map_spool)? {
            if previous
                .is_some_and(|left: PackIndexEntryV1| left.compare_key(&entry) != Ordering::Less)
            {
                return Err(CoreError::NonCanonicalOrder);
            }
            self.append_pack(&encode_index_entry(entry))?;
            previous = Some(entry);
            emitted = emitted.checked_add(1).ok_or(CoreError::IntegerOverflow)?;
        }
        if emitted != self.record_count {
            return Err(CoreError::PackInvalid);
        }
        self.append_pack(&encode_trailer_prefix(
            pack_len,
            index_offset,
            index_len,
            self.record_count,
        ))?;
        let checksum_len = pack_len.checked_sub(32).ok_or(CoreError::IntegerOverflow)?;
        let before_hash_reads = counters.bytes_read;
        let digest = hash_port_range(self.pack, 0, checksum_len, TAG_PACK, self.left, counters)?;
        let hashed_reads = counters
            .bytes_read
            .checked_sub(before_hash_reads)
            .ok_or(CoreError::IntegerOverflow)?;
        self.direct_read_bytes = self
            .direct_read_bytes
            .checked_add(hashed_reads)
            .ok_or(CoreError::IntegerOverflow)?;
        self.direct_read_calls = self
            .direct_read_calls
            .checked_add(checksum_len.div_ceil(COMPARISON_WINDOW_BYTES as u64))
            .ok_or(CoreError::IntegerOverflow)?;
        self.append_pack(&digest)?;
        let id = PackIdV1::from_digest(digest);
        self.pack.seal_direct_v1(id).map_err(map_pack_write)?;
        self.active = false;
        counters.add(CounterFieldV1::PackEntries, u64::from(self.record_count))?;
        counters.add(CounterFieldV1::PackBytes, pack_len)?;
        counters.record_fscas_write(self.direct_write_bytes)?;
        counters.record_fscas_read(self.direct_read_bytes, self.direct_read_calls)?;
        counters.record_pack_storage(0, pack_len)?;
        Ok(SealedPackV1::from_validated_parts(
            id,
            pack_len,
            self.record_count,
            index_offset,
        ))
    }
}

impl<M: PackIndexSpoolV1 + ?Sized> PreparedObjectSinkV1 for DirectPackSinkV1<'_, M> {
    fn resident_memory_bound_bytes(&self) -> CoreResult<u64> {
        u64::try_from(core::mem::size_of::<Self>()).map_err(|_| CoreError::IntegerOverflow)
    }

    fn begin_closure(&mut self) -> Result<(), PreparedSinkErrorV1> {
        if self.active {
            return Err(PreparedSinkErrorV1::Refused);
        }
        self.pack
            .begin_direct_v1()
            .map_err(|_| PreparedSinkErrorV1::Refused)?;
        self.direct_write_bytes = 64;
        self.metadata
            .reset(self.maximum_records)
            .map_err(|_| PreparedSinkErrorV1::Refused)?;
        self.seen_filter.fill(0);
        self.active = true;
        Ok(())
    }

    fn begin_object(
        &mut self,
        kind: PhysicalObjectKindV1,
        complete_len: u64,
    ) -> Result<(), PreparedSinkErrorV1> {
        self.begin_object_inner(kind, complete_len)
            .map_err(|_| PreparedSinkErrorV1::Refused)
    }

    fn write_private(&mut self, bytes: &[u8]) -> Result<(), PreparedSinkErrorV1> {
        self.write_inner(bytes)
            .map_err(|_| PreparedSinkErrorV1::Refused)
    }

    fn finish_object(
        &mut self,
        expected_id: TypedPhysicalObjectIdV1,
    ) -> Result<ObjectDispositionV1, PreparedSinkErrorV1> {
        self.finish_object_inner(expected_id)
            .map_err(|_| PreparedSinkErrorV1::Refused)
    }

    fn finish_closure(&mut self, _result: PreparedFileV1) -> Result<(), PreparedSinkErrorV1> {
        if self.active && self.current.is_none() {
            Ok(())
        } else {
            Err(PreparedSinkErrorV1::Refused)
        }
    }

    fn abort_closure(&mut self) {
        self.current = None;
        self.active = false;
        self.metadata.abort();
        self.pack.abort_private();
    }
}

impl<M: PackIndexSpoolV1 + ?Sized> PreparedTreeSinkV1 for DirectPackSinkV1<'_, M> {
    fn resident_memory_bound_bytes(&self) -> CoreResult<u64> {
        PreparedObjectSinkV1::resident_memory_bound_bytes(self)
    }

    fn begin_private_tree_set(&mut self, _maximum_objects: u32) -> Result<(), TreeSinkErrorV1> {
        if self.active && self.current.is_none() {
            Ok(())
        } else {
            Err(TreeSinkErrorV1::Failure)
        }
    }

    fn admit_private_tree(
        &mut self,
        id: PhysicalTreeIdV1,
        canonical_bytes: &[u8],
    ) -> Result<TreeObjectDispositionV1, TreeSinkErrorV1> {
        self.begin_object_inner(PhysicalObjectKindV1::Tree, canonical_bytes.len() as u64)
            .and_then(|()| self.write_inner(canonical_bytes))
            .and_then(|()| self.finish_object_inner(TypedPhysicalObjectIdV1::Tree(id)))
            .map(|disposition| match disposition {
                ObjectDispositionV1::Created => TreeObjectDispositionV1::Created,
                ObjectDispositionV1::Reused => TreeObjectDispositionV1::Reused,
            })
            .map_err(|_| TreeSinkErrorV1::Failure)
    }

    fn finish_private_tree_set(&mut self, _root: PhysicalTreeIdV1) -> Result<(), TreeSinkErrorV1> {
        if self.active && self.current.is_none() {
            Ok(())
        } else {
            Err(TreeSinkErrorV1::Failure)
        }
    }

    fn abort_private_tree_set(&mut self) {
        PreparedObjectSinkV1::abort_closure(self);
    }
}

const fn kind_index(kind: PhysicalObjectKindV1) -> usize {
    match kind {
        PhysicalObjectKindV1::VersionRecord => 0,
        PhysicalObjectKindV1::Tree => 1,
        PhysicalObjectKindV1::File => 2,
        PhysicalObjectKindV1::Symlink => 3,
        PhysicalObjectKindV1::Chunk => 4,
    }
}

fn map_pack_write(_: PackPortErrorV1) -> CoreError {
    CoreError::SinkRefused
}

fn map_pack_read(_: PackPortErrorV1) -> CoreError {
    CoreError::SourceFailure
}

fn map_spool(_: PackPortErrorV1) -> CoreError {
    CoreError::ResourceRefused
}

pub struct FileChunkReferenceSpoolV1 {
    path: PathBuf,
    file: File,
    maximum: u64,
    count: u64,
    cursor: u64,
}

impl FileChunkReferenceSpoolV1 {
    pub fn create(path: &Path) -> CoreResult<Self> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(path)
            .map_err(|_| CoreError::SinkRefused)?;
        Ok(Self {
            path: path.to_path_buf(),
            file,
            maximum: 0,
            count: 0,
            cursor: 0,
        })
    }

    pub const fn storage_bytes(&self) -> u64 {
        self.count * CHUNK_REFERENCE_RECORD_BYTES
    }
}

impl ChunkReferenceSpoolV1 for FileChunkReferenceSpoolV1 {
    fn resident_memory_bound_bytes(&self, _maximum_refs: u64) -> CoreResult<u64> {
        u64::try_from(core::mem::size_of::<Self>() + self.path.capacity())
            .map_err(|_| CoreError::IntegerOverflow)
    }

    fn storage_bytes_observation(&self) -> CoreResult<Option<u64>> {
        Ok(Some(self.storage_bytes()))
    }

    fn begin(&mut self, maximum_refs: u64) -> Result<(), PreparedSinkErrorV1> {
        self.file
            .set_len(0)
            .map_err(|_| PreparedSinkErrorV1::Refused)?;
        self.maximum = maximum_refs;
        self.count = 0;
        self.cursor = 0;
        Ok(())
    }

    fn push(&mut self, chunk: PreparedChunkRefV1) -> Result<(), PreparedSinkErrorV1> {
        if self.count >= self.maximum {
            return Err(PreparedSinkErrorV1::Refused);
        }
        let mut bytes = [0_u8; CHUNK_REFERENCE_RECORD_BYTES as usize];
        bytes[..32].copy_from_slice(chunk.logical_id().as_bytes());
        bytes[32..64].copy_from_slice(chunk.physical_id().as_bytes());
        bytes[64..68].copy_from_slice(&chunk.len().to_be_bytes());
        self.file
            .seek(SeekFrom::Start(self.count * CHUNK_REFERENCE_RECORD_BYTES))
            .and_then(|_| self.file.write_all(&bytes))
            .map_err(|_| PreparedSinkErrorV1::Refused)?;
        self.count += 1;
        Ok(())
    }

    fn rewind(&mut self) -> Result<(), PreparedSinkErrorV1> {
        self.cursor = 0;
        Ok(())
    }

    fn next(&mut self) -> Result<Option<PreparedChunkRefV1>, PreparedSinkErrorV1> {
        if self.cursor >= self.count {
            return Ok(None);
        }
        let mut bytes = [0_u8; CHUNK_REFERENCE_RECORD_BYTES as usize];
        self.file
            .seek(SeekFrom::Start(self.cursor * CHUNK_REFERENCE_RECORD_BYTES))
            .and_then(|_| self.file.read_exact(&mut bytes))
            .map_err(|_| PreparedSinkErrorV1::Refused)?;
        self.cursor += 1;
        Ok(Some(PreparedChunkRefV1::from_parts(
            LogicalChunkIdV1::from_digest(bytes[..32].try_into().expect("fixed id")),
            PhysicalChunkIdV1::from_digest(bytes[32..64].try_into().expect("fixed id")),
            u32::from_be_bytes(bytes[64..68].try_into().expect("fixed length")),
        )))
    }

    fn abort(&mut self) {
        let _ = self.file.set_len(0);
        self.maximum = 0;
        self.count = 0;
        self.cursor = 0;
    }
}

impl Drop for FileChunkReferenceSpoolV1 {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

pub struct FilePackIndexSpoolV1 {
    path: PathBuf,
    file: File,
    maximum: u32,
    count: u32,
    cursor: u32,
}

impl FilePackIndexSpoolV1 {
    pub fn create(path: &Path) -> CoreResult<Self> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(path)
            .map_err(|_| CoreError::SinkRefused)?;
        Ok(Self {
            path: path.to_path_buf(),
            file,
            maximum: 0,
            count: 0,
            cursor: 0,
        })
    }

    pub const fn storage_bytes(&self) -> u64 {
        self.count as u64 * PACK_INDEX_ENTRY_BYTES
    }

    fn read_entry(&mut self, ordinal: u32) -> Result<PackIndexEntryV1, PackPortErrorV1> {
        if ordinal >= self.count {
            return Err(PackPortErrorV1::Failure);
        }
        let mut bytes = [0_u8; PACK_INDEX_ENTRY_BYTES as usize];
        self.file
            .seek(SeekFrom::Start(u64::from(ordinal) * PACK_INDEX_ENTRY_BYTES))
            .and_then(|_| self.file.read_exact(&mut bytes))
            .map_err(|_| PackPortErrorV1::Failure)?;
        crate::pack::decode_index_entry(&bytes).map_err(|_| PackPortErrorV1::Failure)
    }

    fn write_entry(
        &mut self,
        ordinal: u32,
        entry: PackIndexEntryV1,
    ) -> Result<(), PackPortErrorV1> {
        self.file
            .seek(SeekFrom::Start(u64::from(ordinal) * PACK_INDEX_ENTRY_BYTES))
            .and_then(|_| self.file.write_all(&encode_index_entry(entry)))
            .map_err(|_| PackPortErrorV1::Failure)
    }

    fn sort(&mut self, order: SpoolOrderV1) -> Result<(), PackPortErrorV1> {
        let count = self.count;
        for start in (0..count / 2).rev() {
            self.sift_down(start, count, order)?;
        }
        for end in (1..count).rev() {
            let first = self.read_entry(0)?;
            let last = self.read_entry(end)?;
            self.write_entry(0, last)?;
            self.write_entry(end, first)?;
            self.sift_down(0, end, order)?;
        }
        self.cursor = 0;
        Ok(())
    }

    fn sift_down(
        &mut self,
        mut root: u32,
        end: u32,
        order: SpoolOrderV1,
    ) -> Result<(), PackPortErrorV1> {
        loop {
            let left = root
                .checked_mul(2)
                .and_then(|value| value.checked_add(1))
                .ok_or(PackPortErrorV1::Failure)?;
            if left >= end {
                return Ok(());
            }
            let right = left + 1;
            let mut child = left;
            let mut child_entry = self.read_entry(left)?;
            if right < end {
                let right_entry = self.read_entry(right)?;
                if compare_entry(child_entry, right_entry, order) == Ordering::Less {
                    child = right;
                    child_entry = right_entry;
                }
            }
            let root_entry = self.read_entry(root)?;
            if compare_entry(root_entry, child_entry, order) != Ordering::Less {
                return Ok(());
            }
            self.write_entry(root, child_entry)?;
            self.write_entry(child, root_entry)?;
            root = child;
        }
    }
}

#[derive(Clone, Copy)]
enum SpoolOrderV1 {
    Key,
    Offset,
}

fn compare_entry(left: PackIndexEntryV1, right: PackIndexEntryV1, order: SpoolOrderV1) -> Ordering {
    match order {
        SpoolOrderV1::Key => left.compare_key(&right),
        SpoolOrderV1::Offset => left.compare_offset(&right),
    }
}

impl PackIndexSpoolV1 for FilePackIndexSpoolV1 {
    fn resident_memory_bound_bytes(&self, _maximum_entries: u32) -> CoreResult<u64> {
        u64::try_from(core::mem::size_of::<Self>() + self.path.capacity())
            .map_err(|_| CoreError::IntegerOverflow)
    }

    fn storage_bytes_observation(&self) -> CoreResult<Option<u64>> {
        Ok(Some(self.storage_bytes()))
    }

    fn reset(&mut self, maximum_entries: u32) -> Result<(), PackPortErrorV1> {
        self.file.set_len(0).map_err(|_| PackPortErrorV1::Failure)?;
        self.maximum = maximum_entries;
        self.count = 0;
        self.cursor = 0;
        Ok(())
    }

    fn push(&mut self, entry: PackIndexEntryV1) -> Result<(), PackPortErrorV1> {
        if self.count >= self.maximum {
            return Err(PackPortErrorV1::Failure);
        }
        self.write_entry(self.count, entry)?;
        self.count += 1;
        Ok(())
    }

    fn sort_by_key(&mut self) -> Result<(), PackPortErrorV1> {
        self.sort(SpoolOrderV1::Key)
    }

    fn sort_by_offset(&mut self) -> Result<(), PackPortErrorV1> {
        self.sort(SpoolOrderV1::Offset)
    }

    fn rewind(&mut self) -> Result<(), PackPortErrorV1> {
        self.cursor = 0;
        Ok(())
    }

    fn next(&mut self) -> Result<Option<PackIndexEntryV1>, PackPortErrorV1> {
        if self.cursor >= self.count {
            return Ok(None);
        }
        let entry = self.read_entry(self.cursor)?;
        self.cursor += 1;
        Ok(Some(entry))
    }

    fn abort(&mut self) {
        let _ = self.file.set_len(0);
        self.maximum = 0;
        self.count = 0;
        self.cursor = 0;
    }
}

impl Drop for FilePackIndexSpoolV1 {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}
