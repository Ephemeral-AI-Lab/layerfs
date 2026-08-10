//! Complete bounded carrier writer and index mechanics.
//!
//! This pack-owned implementation receives already-created private carrier,
//! locator, closure, and occupied ports. It cannot reserve an operation slot
//! or create outer preparation state.

use core::cmp::Ordering;
use std::cell::RefCell;

use crate::cas::{
    ClosureObjectRecordV1, FileClosureObjectSpoolV1, FileGlobalSeenSpoolV1, FsCasControlV1,
    FsCasErrorV1, FsCasOccupiedV1, FsCasV1, FsPackAdmissionOutcomeV1, FsPrivatePackV1,
    FsStorageOperationTokenV1, GlobalSeenErrorV1, GlobalSeenLookupV1, GlobalSeenRecordV1,
};
use crate::cdc::CdcControlV1;
use crate::content::{
    ObjectDispositionV1, PreparedFileV1, PreparedObjectSinkV1, PreparedSinkErrorV1,
};
use crate::cow::{PreparedTreeSinkV1, TreeObjectDispositionV1, TreeSinkErrorV1};
use crate::format::{validate_physical_object_len, PhysicalObjectKindV1};
use crate::identity::{
    derive_physical_version_record_id_v1, FramedHasherV1, ObjectChecksumV1, PackIdV1,
    PhysicalTreeIdV1, PhysicalVersionRecordIdV1, COMPARISON_WINDOW_BYTES, TAG_OBJECT_CHECKSUM,
    TAG_PACK,
};
use crate::lifecycle::{SharedC3ControlV1, VersionSummaryInputV1};
use crate::limits::{CounterFieldV1, OperationCountersV1, ResourceLedgerV1};
use crate::object::{
    encode_physical_object_header_v1, TypedPhysicalObjectIdV1, VERSION_RECORD_PAYLOAD_BYTES,
};
use crate::profile::{ChunkerSpecV1, DigestSpecV1};
use crate::{CoreError, CoreResult};

use super::{
    encode_header, encode_index_entry, encode_trailer_prefix, hash_port_range, record_padding,
    CompletedPackSetV1, PackIndexEntryV1, PackIndexSpoolV1, PackPortErrorV1, PackReadPortV1,
    PrivatePackPortV1, SealedPackV1, MAX_PACK_BYTES, MAX_PACK_RECORDS, PACK_INDEX_ENTRY_BYTES,
    PACK_TRAILER_BYTES,
};

const VERSION_OBJECT_BYTES: usize = 52 + VERSION_RECORD_PAYLOAD_BYTES as usize;

struct CurrentObjectV1 {
    kind: PhysicalObjectKindV1,
    record_offset: u64,
    complete_len: u64,
    written: u64,
    checksum: FramedHasherV1,
}

struct CountedPackReadV1<'pack> {
    pack: &'pack mut FsPrivatePackV1,
    bytes_read: u64,
    read_calls: u64,
}

impl PackReadPortV1 for CountedPackReadV1<'_> {
    fn resident_memory_bound_bytes(&self) -> CoreResult<u64> {
        self.pack.resident_memory_bound_bytes()
    }

    fn len(&mut self) -> Result<u64, PackPortErrorV1> {
        self.pack.len()
    }

    fn read_exact_at(
        &mut self,
        offset: u64,
        destination: &mut [u8],
    ) -> Result<(), PackPortErrorV1> {
        self.pack.read_exact_at(offset, destination)?;
        self.bytes_read = self
            .bytes_read
            .checked_add(u64::try_from(destination.len()).map_err(|_| PackPortErrorV1::Failure)?)
            .ok_or(PackPortErrorV1::Failure)?;
        self.read_calls = self
            .read_calls
            .checked_add(1)
            .ok_or(PackPortErrorV1::Failure)?;
        Ok(())
    }
}

pub(crate) struct DirectPackSinkV1<'operation, 'ledger, 'control, M: ?Sized, C: ?Sized> {
    cas: &'operation FsCasV1,
    storage_token: FsStorageOperationTokenV1,
    pack: FsPrivatePackV1,
    metadata: &'operation mut M,
    closure_objects: &'operation mut FileClosureObjectSpoolV1,
    global_seen: &'operation mut FileGlobalSeenSpoolV1,
    occupied: FsCasOccupiedV1,
    left: &'operation mut [u8; COMPARISON_WINDOW_BYTES],
    right: &'operation mut [u8; COMPARISON_WINDOW_BYTES],
    ledger: &'operation ResourceLedgerV1,
    reservation: &'operation crate::limits::OperationReservationV1<'ledger>,
    control: &'operation RefCell<&'control mut C>,
    maximum_records: u32,
    private_pack_resident_bound: u64,
    record_count: u32,
    current: Option<CurrentObjectV1>,
    active: bool,
    direct_write_bytes: u64,
    direct_read_bytes: u64,
    direct_read_calls: u64,
    storage_counters: OperationCountersV1,
    carrier_count: u32,
    carriers_installed: u32,
    carriers_reused: u32,
    installed_residue_bytes: u64,
    index_spool_bytes: Option<u64>,
    last_admission: Option<(SealedPackV1, FsPackAdmissionOutcomeV1)>,
    first_fscas_error: Option<FsCasErrorV1>,
}

impl<'operation, 'ledger, 'control, M, C> DirectPackSinkV1<'operation, 'ledger, 'control, M, C>
where
    M: PackIndexSpoolV1 + ?Sized,
    C: CdcControlV1 + FsCasControlV1 + ?Sized,
{
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        cas: &'operation FsCasV1,
        storage_token: FsStorageOperationTokenV1,
        pack: FsPrivatePackV1,
        metadata: &'operation mut M,
        closure_objects: &'operation mut FileClosureObjectSpoolV1,
        global_seen: &'operation mut FileGlobalSeenSpoolV1,
        occupied: FsCasOccupiedV1,
        left: &'operation mut [u8; COMPARISON_WINDOW_BYTES],
        right: &'operation mut [u8; COMPARISON_WINDOW_BYTES],
        maximum_records: u32,
        private_pack_resident_bound: u64,
        ledger: &'operation ResourceLedgerV1,
        reservation: &'operation crate::limits::OperationReservationV1<'ledger>,
        control: &'operation RefCell<&'control mut C>,
    ) -> Self {
        Self {
            cas,
            storage_token,
            pack,
            metadata,
            closure_objects,
            global_seen,
            occupied,
            left,
            right,
            ledger,
            reservation,
            control,
            maximum_records,
            private_pack_resident_bound,
            record_count: 0,
            current: None,
            active: false,
            direct_write_bytes: 0,
            direct_read_bytes: 0,
            direct_read_calls: 0,
            storage_counters: OperationCountersV1::default(),
            carrier_count: 0,
            carriers_installed: 0,
            carriers_reused: 0,
            installed_residue_bytes: 0,
            index_spool_bytes: Some(0),
            last_admission: None,
            first_fscas_error: None,
        }
    }

    fn closure_objects_mut(&mut self) -> CoreResult<&mut FileClosureObjectSpoolV1> {
        Ok(&mut *self.closure_objects)
    }

    fn global_seen_mut(&mut self) -> CoreResult<&mut FileGlobalSeenSpoolV1> {
        Ok(&mut *self.global_seen)
    }

    fn map_global_seen_error(&mut self, error: GlobalSeenErrorV1) -> CoreError {
        match error {
            GlobalSeenErrorV1::Core(error) => error,
            GlobalSeenErrorV1::FsCas(error) => {
                self.first_fscas_error.get_or_insert(error);
                map_fscas_operation(error)
            }
        }
    }

    fn map_occupied_fscas_error(&mut self, error: FsCasErrorV1) -> CoreError {
        self.first_fscas_error.get_or_insert(error);
        map_fscas_operation(error)
    }

    fn lookup_global_seen_v1(
        &mut self,
        id: TypedPhysicalObjectIdV1,
    ) -> CoreResult<GlobalSeenLookupV1> {
        let mut shared_control = SharedC3ControlV1::new(self.control);
        let lookup = self.global_seen.lookup(id, &mut shared_control);
        lookup.map_err(|error| self.map_global_seen_error(error))
    }

    fn begin_current_carrier_v1(&mut self) -> CoreResult<()> {
        let result = {
            let mut control = SharedC3ControlV1::new(self.control);
            self.pack
                .begin_direct_controlled_v1(MAX_PACK_BYTES, &mut control)
        };
        result.map_err(|error| {
            map_private_pack_error_v1(
                &mut self.pack,
                &mut self.first_fscas_error,
                error,
                CoreError::SinkRefused,
            )
        })?;
        self.direct_write_bytes = crate::pack::PACK_HEADER_BYTES;
        self.direct_read_bytes = 0;
        self.direct_read_calls = 0;
        let carrier_record_cap =
            u32::try_from(MAX_PACK_RECORDS).map_err(|_| CoreError::IntegerOverflow)?;
        self.metadata
            .reset(self.maximum_records.min(carrier_record_cap))
            .map_err(map_spool)?;
        self.record_count = 0;
        self.active = true;
        Ok(())
    }

    fn start_next_carrier_v1(&mut self) -> CoreResult<()> {
        let mut next_pack = match self.cas.begin_private_pack_borrowed_v1(self.storage_token) {
            Ok(pack) => pack,
            Err(error) => {
                self.first_fscas_error.get_or_insert(error);
                return Err(map_fscas_operation(error));
            }
        };
        let resident = match next_pack.resident_memory_bound_bytes() {
            Ok(resident) => resident,
            Err(error) => {
                let mut shared_control = SharedC3ControlV1::new(self.control);
                if let Err(cleanup_error) = next_pack.cleanup_controlled_v1(&mut shared_control) {
                    self.first_fscas_error.get_or_insert(cleanup_error);
                    return Err(map_fscas_operation(cleanup_error));
                }
                return Err(error);
            }
        };
        if resident > self.private_pack_resident_bound {
            let mut shared_control = SharedC3ControlV1::new(self.control);
            if let Err(error) = next_pack.cleanup_controlled_v1(&mut shared_control) {
                self.first_fscas_error.get_or_insert(error);
                return Err(map_fscas_operation(error));
            }
            return Err(CoreError::ResourceRefused);
        }
        self.pack = next_pack;
        self.begin_current_carrier_v1()
    }

    fn seal_admit_current_v1(&mut self) -> CoreResult<()> {
        let mut carrier_counters = OperationCountersV1::default();
        let sealed = self.finalize_v1(&mut carrier_counters)?;
        carrier_counters.record_carrier(sealed.pack_len(), self.carrier_count != 0)?;
        let observed_index = self.metadata.storage_bytes_observation()?;
        self.index_spool_bytes = match (self.index_spool_bytes, observed_index) {
            (Some(total), Some(bytes)) => {
                Some(total.checked_add(bytes).ok_or(CoreError::IntegerOverflow)?)
            }
            _ => None,
        };
        let admission = {
            let mut shared_control = SharedC3ControlV1::new(self.control);
            self.cas.admit_pack_borrowed_controlled_v1(
                &mut self.pack,
                self.metadata,
                self.ledger,
                self.reservation,
                self.storage_token,
                &mut carrier_counters,
                self.left,
                &mut shared_control,
            )
        };
        self.storage_counters.accumulate(carrier_counters)?;
        let admission = match admission {
            Ok(admission) => admission,
            Err(error) => {
                self.first_fscas_error.get_or_insert(error);
                return Err(map_fscas_operation(error));
            }
        };
        if admission.sealed() != sealed {
            return Err(CoreError::PackInvalid);
        }
        self.carrier_count = self
            .carrier_count
            .checked_add(1)
            .ok_or(CoreError::IntegerOverflow)?;
        match admission.outcome() {
            FsPackAdmissionOutcomeV1::Installed => {
                self.carriers_installed = self
                    .carriers_installed
                    .checked_add(1)
                    .ok_or(CoreError::IntegerOverflow)?;
                self.installed_residue_bytes = self
                    .installed_residue_bytes
                    .checked_add(sealed.pack_len())
                    .ok_or(CoreError::IntegerOverflow)?;
            }
            FsPackAdmissionOutcomeV1::ExistingComplete => {
                self.carriers_reused = self
                    .carriers_reused
                    .checked_add(1)
                    .ok_or(CoreError::IntegerOverflow)?;
            }
        }
        self.last_admission = Some((sealed, admission.outcome()));
        Ok(())
    }

    pub(crate) fn complete_v1(
        &mut self,
        expected_version: PhysicalVersionRecordIdV1,
    ) -> CoreResult<CompletedPackSetV1> {
        self.seal_admit_current_v1()?;
        // Construction may encounter the same typed object more than once.
        // The closure spool is intentionally canonicalized to unique ids;
        // `stats.count` is the post-canonical closure count, not a comparison
        // against the already-mutated spool count.
        let stats = {
            let mut shared_control = SharedC3ControlV1::new(self.control);
            self.closure_objects
                .sort_unique(&mut shared_control, &mut self.storage_counters)?
        };
        if stats.count == 0
            || self.closure_objects_mut()?.read(0)?.id
                != TypedPhysicalObjectIdV1::VersionRecord(expected_version)
        {
            return Err(CoreError::PackInvalid);
        }
        let (last_sealed, last_outcome) = self.last_admission.ok_or(CoreError::PackInvalid)?;
        Ok(CompletedPackSetV1 {
            last_sealed,
            last_outcome,
            carrier_count: self.carrier_count,
            carriers_installed: self.carriers_installed,
            carriers_reused: self.carriers_reused,
            installed_residue_bytes: self.installed_residue_bytes,
            index_spool_bytes: self.index_spool_bytes,
        })
    }

    /// Make every changed candidate object readable through the real FsCas
    /// before the operation reconstructs the complete candidate closure.
    /// An all-reuse mutation leaves the already-open empty carrier in place;
    /// otherwise the changed-object carrier is sealed/admitted and a fresh
    /// bounded carrier is opened for the version record.
    pub(crate) fn flush_changed_objects_for_candidate_v1(&mut self) -> CoreResult<()> {
        if !self.active || self.current.is_some() {
            return Err(CoreError::SinkRefused);
        }
        if self.record_count != 0 {
            self.seal_admit_current_v1()?;
            self.start_next_carrier_v1()?;
        }
        Ok(())
    }

    /// Narrow CAS-owned access to the already-granted preparation ports used
    /// to discover a complete candidate graph. These ports cannot install a
    /// catalog or closure marker and remain borrowed by the one outer
    /// lifecycle operation.
    pub(crate) fn candidate_graph_parts_v1(
        &mut self,
    ) -> (
        &mut FileClosureObjectSpoolV1,
        &mut FileGlobalSeenSpoolV1,
        &mut FsCasOccupiedV1,
        &mut [u8; COMPARISON_WINDOW_BYTES],
    ) {
        (
            &mut *self.closure_objects,
            &mut *self.global_seen,
            &mut self.occupied,
            self.right,
        )
    }

    pub(crate) fn record_incomplete_residue(&mut self) -> CoreResult<()> {
        if self.installed_residue_bytes != 0 {
            self.storage_counters
                .record_unreachable_installed_residue(self.installed_residue_bytes)?;
        }
        Ok(())
    }

    pub(crate) fn take_storage_counters(&mut self) -> OperationCountersV1 {
        core::mem::take(&mut self.storage_counters)
    }

    pub(crate) fn take_first_fscas_error(&mut self) -> Option<FsCasErrorV1> {
        self.first_fscas_error.take()
    }

    pub(crate) fn cleanup_private_pack_controlled_v1(&mut self) -> Result<(), FsCasErrorV1> {
        let mut shared_control = SharedC3ControlV1::new(self.control);
        let result = self.pack.cleanup_controlled_v1(&mut shared_control);
        if let Err(error) = result {
            self.first_fscas_error.get_or_insert(error);
        }
        result
    }

    pub(crate) fn record_global_seen_observation(&mut self) -> CoreResult<()> {
        let table = &*self.global_seen;
        let (lookups, probes, maximum_probe, entries) = table.work_observation();
        let (metadata_bytes_read, metadata_read_calls, metadata_bytes_written) =
            table.direct_storage_observation();
        self.storage_counters.record_global_seen(
            lookups,
            probes,
            maximum_probe,
            entries,
            table.storage_bytes(),
            metadata_bytes_read,
            metadata_read_calls,
            metadata_bytes_written,
        )?;
        Ok(())
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
        let record_bytes = 4_u64
            .checked_add(complete_len)
            .and_then(|bytes| {
                record_padding(complete_len)
                    .ok()
                    .map(u64::from)
                    .and_then(|padding| bytes.checked_add(padding))
            })
            .ok_or(CoreError::IntegerOverflow)?;
        let next_record_count = self
            .record_count
            .checked_add(1)
            .ok_or(CoreError::IntegerOverflow)?;
        let projected = self
            .pack
            .len()
            .map_err(|error| {
                map_private_pack_error_v1(
                    &mut self.pack,
                    &mut self.first_fscas_error,
                    error,
                    CoreError::SourceFailure,
                )
            })?
            .checked_add(record_bytes)
            .and_then(|bytes| {
                u64::from(next_record_count)
                    .checked_mul(PACK_INDEX_ENTRY_BYTES)
                    .and_then(|index| bytes.checked_add(index))
            })
            .and_then(|bytes| bytes.checked_add(PACK_TRAILER_BYTES))
            .ok_or(CoreError::IntegerOverflow)?;
        if u64::from(next_record_count) > MAX_PACK_RECORDS || projected > MAX_PACK_BYTES {
            if self.record_count == 0 {
                return Err(CoreError::ResourceRefused);
            }
            self.seal_admit_current_v1()?;
            self.start_next_carrier_v1()?;
        }
        let record_offset = self.pack.len().map_err(|error| {
            map_private_pack_error_v1(
                &mut self.pack,
                &mut self.first_fscas_error,
                error,
                CoreError::SourceFailure,
            )
        })?;
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
        let result = {
            let mut control = SharedC3ControlV1::new(self.control);
            self.pack.append_controlled_v1(bytes, &mut control)
        };
        result.map_err(|error| {
            map_private_pack_error_v1(
                &mut self.pack,
                &mut self.first_fscas_error,
                error,
                CoreError::SinkRefused,
            )
        })?;
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
        let lookup = self.lookup_global_seen_v1(expected_id)?;
        if let Some(incumbent) = lookup.record {
            if incumbent.complete_len != current.complete_len {
                return Err(CoreError::IdMismatch);
            }
            let same_carrier = incumbent.carrier_ordinal == self.carrier_count;
            let equal = if same_carrier {
                self.compare_objects(
                    incumbent.private_payload_offset,
                    current.record_offset + 4,
                    current.complete_len,
                )?
            } else {
                self.compare_occupied_object_v1(
                    expected_id,
                    current.record_offset + 4,
                    current.complete_len,
                )?
            };
            if !equal {
                return Err(CoreError::IdMismatch);
            }
            self.storage_counters.add(
                if same_carrier {
                    CounterFieldV1::GlobalSeenSameCarrierReuses
                } else {
                    CounterFieldV1::GlobalSeenCrossCarrierReuses
                },
                1,
            )?;
            self.pack
                .truncate_direct_v1(current.record_offset)
                .map_err(|error| {
                    map_private_pack_error_v1(
                        &mut self.pack,
                        &mut self.first_fscas_error,
                        error,
                        CoreError::SinkRefused,
                    )
                })?;
            self.closure_objects_mut()?.push(ClosureObjectRecordV1 {
                id: expected_id,
                complete_len: current.complete_len,
            })?;
            self.storage_counters
                .record_pack_object_disposition(current.kind, false)?;
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
        let carrier_ordinal = self.carrier_count;
        let mut shared_control = SharedC3ControlV1::new(self.control);
        if let Err(error) = self.global_seen.insert_controlled_v1(
            lookup.vacant_slot,
            expected_id,
            GlobalSeenRecordV1 {
                complete_len: current.complete_len,
                private_payload_offset: current.record_offset + 4,
                carrier_ordinal,
            },
            &mut shared_control,
        ) {
            return Err(self.map_global_seen_error(error));
        }
        self.record_count = self
            .record_count
            .checked_add(1)
            .ok_or(CoreError::IntegerOverflow)?;
        self.closure_objects_mut()?.push(ClosureObjectRecordV1 {
            id: expected_id,
            complete_len: current.complete_len,
        })?;
        self.storage_counters
            .record_pack_object_disposition(current.kind, true)?;
        Ok(ObjectDispositionV1::Created)
    }

    fn compare_objects(&mut self, left: u64, right: u64, len: u64) -> CoreResult<bool> {
        let mut offset = 0_u64;
        while offset < len {
            let take = usize::try_from((len - offset).min(COMPARISON_WINDOW_BYTES as u64))
                .map_err(|_| CoreError::IntegerOverflow)?;
            self.pack
                .read_exact_at(left + offset, &mut self.left[..take])
                .map_err(|error| {
                    map_private_pack_error_v1(
                        &mut self.pack,
                        &mut self.first_fscas_error,
                        error,
                        CoreError::SourceFailure,
                    )
                })?;
            self.pack
                .read_exact_at(right + offset, &mut self.right[..take])
                .map_err(|error| {
                    map_private_pack_error_v1(
                        &mut self.pack,
                        &mut self.first_fscas_error,
                        error,
                        CoreError::SourceFailure,
                    )
                })?;
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

    fn compare_occupied_object_v1(
        &mut self,
        id: TypedPhysicalObjectIdV1,
        private_offset: u64,
        len: u64,
    ) -> CoreResult<bool> {
        let before = match self.occupied.direct_storage_read_observation_typed_v1() {
            Ok(observation) => observation,
            Err(error) => return Err(self.map_occupied_fscas_error(error)),
        };
        let result = (|| {
            let occupied_len = match self.occupied.occupied_len_typed_v1(id) {
                Ok(Some(len)) => len,
                Ok(None) => {
                    let error = FsCasErrorV1::Integrity;
                    return Err(self.map_occupied_fscas_error(error));
                }
                Err(error) => return Err(self.map_occupied_fscas_error(error)),
            };
            if occupied_len != len {
                return Err(CoreError::IdMismatch);
            }
            let mut offset = 0_u64;
            while offset < len {
                let take = usize::try_from((len - offset).min(COMPARISON_WINDOW_BYTES as u64))
                    .map_err(|_| CoreError::IntegerOverflow)?;
                if let Err(error) = self.occupied.read_occupied_exact_at_typed_v1(
                    id,
                    offset,
                    &mut self.left[..take],
                ) {
                    return Err(self.map_occupied_fscas_error(error));
                }
                self.pack
                    .read_exact_at(private_offset + offset, &mut self.right[..take])
                    .map_err(|error| {
                        map_private_pack_error_v1(
                            &mut self.pack,
                            &mut self.first_fscas_error,
                            error,
                            CoreError::SourceFailure,
                        )
                    })?;
                self.direct_read_bytes = self
                    .direct_read_bytes
                    .checked_add(take as u64)
                    .ok_or(CoreError::IntegerOverflow)?;
                self.direct_read_calls = self
                    .direct_read_calls
                    .checked_add(1)
                    .ok_or(CoreError::IntegerOverflow)?;
                if self.left[..take] != self.right[..take] {
                    return Ok(false);
                }
                offset = offset
                    .checked_add(take as u64)
                    .ok_or(CoreError::IntegerOverflow)?;
            }
            Ok(true)
        })();
        let after = match self.occupied.direct_storage_read_observation_typed_v1() {
            Ok(observation) => observation,
            Err(error) => return Err(self.map_occupied_fscas_error(error)),
        };
        self.storage_counters.record_fscas_read(
            after
                .0
                .checked_sub(before.0)
                .ok_or(CoreError::IntegerOverflow)?,
            after
                .1
                .checked_sub(before.1)
                .ok_or(CoreError::IntegerOverflow)?,
        )?;
        result
    }

    fn append_pack(&mut self, bytes: &[u8]) -> CoreResult<()> {
        let result = {
            let mut control = SharedC3ControlV1::new(self.control);
            self.pack.append_controlled_v1(bytes, &mut control)
        };
        result.map_err(|error| {
            map_private_pack_error_v1(
                &mut self.pack,
                &mut self.first_fscas_error,
                error,
                CoreError::SinkRefused,
            )
        })?;
        self.direct_write_bytes = self
            .direct_write_bytes
            .checked_add(bytes.len() as u64)
            .ok_or(CoreError::IntegerOverflow)?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn write_version_v1(
        &mut self,
        version_id: crate::identity::VersionIdV1,
        root_tree: PhysicalTreeIdV1,
        summary: VersionSummaryInputV1,
        counters: &mut OperationCountersV1,
    ) -> CoreResult<PhysicalVersionRecordIdV1> {
        let closure_stats = {
            let mut shared_control = SharedC3ControlV1::new(self.control);
            self.closure_objects
                .sort_unique(&mut shared_control, &mut self.storage_counters)?
        };
        let tree_count = closure_stats.kind_counts[kind_index(PhysicalObjectKindV1::Tree)];
        let file_count = closure_stats.kind_counts[kind_index(PhysicalObjectKindV1::File)];
        let total_object_count = closure_stats
            .count
            .checked_add(1)
            .ok_or(CoreError::IntegerOverflow)?;
        let mut object = [0_u8; VERSION_OBJECT_BYTES];
        object[..52].copy_from_slice(&encode_physical_object_header_v1(
            PhysicalObjectKindV1::VersionRecord,
            VERSION_RECORD_PAYLOAD_BYTES,
        ));
        let payload = &mut object[52..];
        payload[0..32].copy_from_slice(version_id.as_bytes());
        payload[32..64].copy_from_slice(ChunkerSpecV1::frozen().id().as_bytes());
        payload[64..96].copy_from_slice(DigestSpecV1::frozen().id().as_bytes());
        payload[96..128].copy_from_slice(root_tree.as_bytes());
        payload[128..136].copy_from_slice(&summary.canonical_len.to_be_bytes());
        payload[136..144].copy_from_slice(&summary.logical_file_bytes.to_be_bytes());
        payload[144..148].copy_from_slice(&summary.entry_count.to_be_bytes());
        payload[148..152].copy_from_slice(&tree_count.to_be_bytes());
        payload[152..156].copy_from_slice(&file_count.to_be_bytes());
        payload[156..160].copy_from_slice(&0_u32.to_be_bytes());
        payload[160..164].copy_from_slice(
            &closure_stats.kind_counts[kind_index(PhysicalObjectKindV1::Chunk)].to_be_bytes(),
        );
        payload[164..168].copy_from_slice(&summary.extent_count.to_be_bytes());
        payload[168..172].copy_from_slice(&summary.chunk_ref_count.to_be_bytes());
        payload[172..176].copy_from_slice(&total_object_count.to_be_bytes());
        payload[176..184].copy_from_slice(&closure_stats.physical_chunk_bytes.to_be_bytes());
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
        let index_offset = self.pack.len().map_err(|error| {
            map_private_pack_error_v1(
                &mut self.pack,
                &mut self.first_fscas_error,
                error,
                CoreError::SourceFailure,
            )
        })?;
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
        let header_result = {
            let mut control = SharedC3ControlV1::new(self.control);
            self.pack.patch_direct_controlled_v1(
                0,
                &encode_header(self.record_count, index_offset),
                &mut control,
            )
        };
        header_result.map_err(|error| {
            map_private_pack_error_v1(
                &mut self.pack,
                &mut self.first_fscas_error,
                error,
                CoreError::SinkRefused,
            )
        })?;
        self.direct_write_bytes = self
            .direct_write_bytes
            .checked_add(64)
            .ok_or(CoreError::IntegerOverflow)?;
        {
            let mut shared_control = SharedC3ControlV1::new(self.control);
            self.metadata
                .sort_by_key_controlled(&mut shared_control, &mut self.storage_counters)
                .map_err(map_spool)?;
        }
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
        let mut counted_pack = CountedPackReadV1 {
            pack: &mut self.pack,
            bytes_read: 0,
            read_calls: 0,
        };
        let digest_result = hash_port_range(
            &mut counted_pack,
            0,
            checksum_len,
            TAG_PACK,
            self.left,
            counters,
        );
        self.direct_read_bytes = self
            .direct_read_bytes
            .checked_add(counted_pack.bytes_read)
            .ok_or(CoreError::IntegerOverflow)?;
        self.direct_read_calls = self
            .direct_read_calls
            .checked_add(counted_pack.read_calls)
            .ok_or(CoreError::IntegerOverflow)?;
        let digest = match digest_result {
            Ok(digest) => digest,
            Err(error) => {
                if let Some(storage_error) = self.pack.take_first_error_typed_v1() {
                    self.first_fscas_error.get_or_insert(storage_error);
                    return Err(map_fscas_operation(storage_error));
                }
                return Err(error);
            }
        };
        self.append_pack(&digest)?;
        let id = PackIdV1::from_digest(digest);
        let seal_result = {
            let mut control = SharedC3ControlV1::new(self.control);
            self.pack.seal_direct_controlled_v1(id, &mut control)
        };
        seal_result.map_err(|error| {
            map_private_pack_error_v1(
                &mut self.pack,
                &mut self.first_fscas_error,
                error,
                CoreError::SinkRefused,
            )
        })?;
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

impl<M, C> PreparedObjectSinkV1 for DirectPackSinkV1<'_, '_, '_, M, C>
where
    M: PackIndexSpoolV1 + ?Sized,
    C: CdcControlV1 + FsCasControlV1 + ?Sized,
{
    fn resident_memory_bound_bytes(&self) -> CoreResult<u64> {
        u64::try_from(core::mem::size_of::<Self>()).map_err(|_| CoreError::IntegerOverflow)
    }

    fn begin_closure(&mut self) -> Result<(), PreparedSinkErrorV1> {
        if self.active {
            return if self.current.is_none() {
                Ok(())
            } else {
                Err(PreparedSinkErrorV1::Refused)
            };
        }
        self.begin_current_carrier_v1()
            .map_err(|_| PreparedSinkErrorV1::Refused)
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

impl<M, C> PreparedTreeSinkV1 for DirectPackSinkV1<'_, '_, '_, M, C>
where
    M: PackIndexSpoolV1 + ?Sized,
    C: CdcControlV1 + FsCasControlV1 + ?Sized,
{
    fn resident_memory_bound_bytes(&self) -> CoreResult<u64> {
        PreparedObjectSinkV1::resident_memory_bound_bytes(self)
    }

    fn begin_private_tree_set(&mut self, _maximum_objects: u32) -> Result<(), TreeSinkErrorV1> {
        if self.active {
            return if self.current.is_none() {
                Ok(())
            } else {
                Err(TreeSinkErrorV1::Failure)
            };
        }
        self.begin_current_carrier_v1()
            .map_err(|_| TreeSinkErrorV1::Failure)
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

fn map_private_pack_error_v1(
    pack: &mut FsPrivatePackV1,
    first_fscas_error: &mut Option<FsCasErrorV1>,
    _error: PackPortErrorV1,
    fallback: CoreError,
) -> CoreError {
    if let Some(error) = pack.take_first_error_typed_v1() {
        first_fscas_error.get_or_insert(error);
        map_fscas_operation(error)
    } else {
        fallback
    }
}

const fn map_spool(error: PackPortErrorV1) -> CoreError {
    match error {
        PackPortErrorV1::Failure | PackPortErrorV1::WorkExhausted => CoreError::ResourceRefused,
        PackPortErrorV1::Cancelled => CoreError::Cancelled,
        PackPortErrorV1::Deadline => CoreError::Deadline,
    }
}

fn map_fscas_operation(error: FsCasErrorV1) -> CoreError {
    match error {
        FsCasErrorV1::Core(error) => error,
        FsCasErrorV1::Unsupported | FsCasErrorV1::Busy | FsCasErrorV1::ResourceExhausted(_) => {
            CoreError::ResourceRefused
        }
        FsCasErrorV1::Invalidated
        | FsCasErrorV1::CrossOwner
        | FsCasErrorV1::CleanupFailed(_)
        | FsCasErrorV1::InvalidationFailed
        | FsCasErrorV1::MalformedOccupant
        | FsCasErrorV1::MissingOccupant
        | FsCasErrorV1::UnequalOccupant
        | FsCasErrorV1::Filesystem(_)
        | FsCasErrorV1::Io
        | FsCasErrorV1::Integrity
        | FsCasErrorV1::Collision => CoreError::SinkRefused,
    }
}
