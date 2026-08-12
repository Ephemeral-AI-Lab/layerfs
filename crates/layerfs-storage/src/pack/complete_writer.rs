//! Complete bounded carrier writer and index mechanics.
//!
//! This pack-owned implementation receives already-created private carrier,
//! locator, closure, and occupied ports. It cannot reserve an operation slot
//! or create outer preparation state.

use core::cmp::Ordering;
use std::cell::RefCell;

use crate::cas::{
    ClosureObjectRecordV1, FileClosureObjectSpoolV1, FileGlobalSeenSpoolV1, FsCasControlV1,
    FsCasErrorV1, FsCasOccupiedV1, FsCasV1, FsOperationSpoolV1, FsPackAdmissionOutcomeV1,
    FsPrivatePackV1, FsStorageOperationTokenV1, GlobalSeenErrorV1, GlobalSeenLookupV1,
    GlobalSeenRecordV1,
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
use crate::lifecycle::{SharedOperationControlV1, VersionSummaryInputV1};
use crate::limits::{
    CounterFieldV1, ObservationScopeV1, OperationCountersV1, OptionalU64ObservationV1,
};
use crate::object::{
    encode_physical_object_header_v1, TypedPhysicalObjectIdV1, VERSION_RECORD_PAYLOAD_BYTES,
};
use crate::profile::{ChunkerSpecV1, DigestSpecV1};
use crate::{CoreError, CoreResult};

use super::{
    encode_header, encode_index_entry, encode_trailer_prefix, hash_port_range_controlled_v1,
    record_padding, CompletedPackSetV1, PackIndexEntryV1, PackIndexSpoolV1, PackPortErrorV1,
    PackReadPortV1, PrivatePackPortV1, SealedPackV1, MAX_PACK_BYTES, MAX_PACK_RECORDS,
    PACK_INDEX_ENTRY_BYTES, PACK_TRAILER_BYTES,
};

const VERSION_OBJECT_BYTES: usize = 52 + VERSION_RECORD_PAYLOAD_BYTES as usize;

struct CurrentObjectV1 {
    kind: PhysicalObjectKindV1,
    record_offset: u64,
    complete_len: u64,
    written: u64,
    checksum: FramedHasherV1,
}

struct CountedPackReadV1<'pack, P: ?Sized> {
    pack: &'pack mut P,
    bytes_read: u64,
    read_calls: u64,
    first_core_error: Option<CoreError>,
}

impl<P: PackReadPortV1 + ?Sized> PackReadPortV1 for CountedPackReadV1<'_, P> {
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
        let amount = match u64::try_from(destination.len()) {
            Ok(amount) => amount,
            Err(_) => {
                self.first_core_error
                    .get_or_insert(CoreError::IntegerOverflow);
                return Err(PackPortErrorV1::Failure);
            }
        };
        let next_bytes_read = match self.bytes_read.checked_add(amount) {
            Some(next) => next,
            None => {
                self.first_core_error
                    .get_or_insert(CoreError::IntegerOverflow);
                return Err(PackPortErrorV1::Failure);
            }
        };
        let next_read_calls = match self.read_calls.checked_add(1) {
            Some(next) => next,
            None => {
                self.first_core_error
                    .get_or_insert(CoreError::IntegerOverflow);
                return Err(PackPortErrorV1::Failure);
            }
        };
        self.bytes_read = next_bytes_read;
        self.read_calls = next_read_calls;
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
    locator_receipts: &'operation mut FsOperationSpoolV1,
    occupied: FsCasOccupiedV1,
    left: &'operation mut [u8; COMPARISON_WINDOW_BYTES],
    right: &'operation mut [u8; COMPARISON_WINDOW_BYTES],
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
    /// Exact newly-installed immutable custody that could not yet join the
    /// normal tally because its post-admission observation failed. There can
    /// be at most one: a tally failure terminates this sink immediately. Keep
    /// it distinct until `record_incomplete_residue` transfers both values as
    /// one checked observation transaction.
    pending_installed_residue_bytes: Option<u64>,
    index_spool_bytes: OptionalU64ObservationV1,
    last_admission: Option<(SealedPackV1, FsPackAdmissionOutcomeV1)>,
    first_core_error: Option<CoreError>,
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
        locator_receipts: &'operation mut FsOperationSpoolV1,
        occupied: FsCasOccupiedV1,
        left: &'operation mut [u8; COMPARISON_WINDOW_BYTES],
        right: &'operation mut [u8; COMPARISON_WINDOW_BYTES],
        maximum_records: u32,
        private_pack_resident_bound: u64,
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
            locator_receipts,
            occupied,
            left,
            right,
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
            pending_installed_residue_bytes: None,
            index_spool_bytes: OptionalU64ObservationV1::observed(
                0,
                "direct cumulative pack-index spool logical length",
                ObservationScopeV1::Operation,
            ),
            last_admission: None,
            first_core_error: None,
            first_fscas_error: None,
        }
    }

    fn retain_prepared_sink_core_error_v1(&mut self, error: CoreError) -> PreparedSinkErrorV1 {
        self.first_core_error.get_or_insert(error);
        PreparedSinkErrorV1::Refused
    }

    fn retain_tree_sink_core_error_v1(&mut self, error: CoreError) -> TreeSinkErrorV1 {
        self.first_core_error.get_or_insert(error);
        TreeSinkErrorV1::Failure
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

    fn poll_control_v1(&mut self) -> CoreResult<()> {
        let mut control = SharedOperationControlV1::new(self.control);
        if crate::limits::OperationWorkControlV1::cancellation_requested_v1(&mut control) {
            Err(CoreError::Cancelled)
        } else if crate::limits::OperationWorkControlV1::deadline_exceeded_v1(&mut control) {
            Err(CoreError::Deadline)
        } else {
            Ok(())
        }
    }

    fn lookup_global_seen_v1(
        &mut self,
        id: TypedPhysicalObjectIdV1,
    ) -> CoreResult<GlobalSeenLookupV1> {
        let mut shared_control = SharedOperationControlV1::new(self.control);
        let lookup = self.global_seen.lookup(id, &mut shared_control);
        lookup.map_err(|error| self.map_global_seen_error(error))
    }

    fn begin_current_carrier_v1(&mut self) -> CoreResult<()> {
        let result = {
            let mut control = SharedOperationControlV1::new(self.control);
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
        if let Err(error) = self
            .metadata
            .reset(self.maximum_records.min(carrier_record_cap))
        {
            return Err(self.promote_metadata_spool_error_v1(error));
        }
        self.record_count = 0;
        self.active = true;
        Ok(())
    }

    fn start_next_carrier_v1(&mut self) -> CoreResult<()> {
        let mut shared_control = SharedOperationControlV1::new(self.control);
        let mut next_pack = match self
            .cas
            .begin_private_pack_borrowed_controlled_v1(self.storage_token, &mut shared_control)
        {
            Ok(pack) => pack,
            Err(error) => {
                self.first_fscas_error.get_or_insert(error);
                return Err(map_fscas_operation(error));
            }
        };
        let resident = match next_pack.resident_memory_bound_bytes() {
            Ok(resident) => resident,
            Err(error) => {
                if let Err(cleanup_error) = next_pack.cleanup_controlled_v1(&mut shared_control) {
                    let terminal = next_private_pack_cleanup_terminal_v1(error, cleanup_error);
                    self.first_fscas_error.get_or_insert(terminal);
                    return Err(map_fscas_operation(terminal));
                }
                return Err(error);
            }
        };
        if resident > self.private_pack_resident_bound {
            if let Err(cleanup_error) = next_pack.cleanup_controlled_v1(&mut shared_control) {
                let terminal = next_private_pack_cleanup_terminal_v1(
                    CoreError::ResourceRefused,
                    cleanup_error,
                );
                self.first_fscas_error.get_or_insert(terminal);
                return Err(map_fscas_operation(terminal));
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
        self.index_spool_bytes = self.index_spool_bytes.checked_add_operation_v1(
            observed_index,
            "direct cumulative pack-index spool logical length",
        )?;
        let admission_terminal = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut shared_control = SharedOperationControlV1::new(self.control);
            self.cas.admit_pack_borrowed_controlled_v1(
                &mut self.pack,
                self.metadata,
                self.reservation,
                self.storage_token,
                &mut carrier_counters,
                self.left,
                self.locator_receipts,
                &mut shared_control,
            )
        }));
        let admission = match admission_terminal {
            Ok(terminal) => terminal,
            Err(payload) => {
                // Admission owns direct carrier write/read/residue events in
                // this per-carrier accumulator. Preserve them before the
                // callback panic leaves the sink; the complete lifecycle
                // observer remains live through cleanup and terminalization.
                match self.accumulate_carrier_counters_v1(carrier_counters) {
                    Ok(()) => std::panic::resume_unwind(payload),
                    Err(error) => return Err(error),
                }
            }
        };
        self.accumulate_carrier_counters_v1(carrier_counters)?;
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
        // Admission can make a carrier, its persistent locators, and its
        // catalog marker visible before this sink updates its local result
        // tally.  Stage *every* tally field first.  If any checked tally
        // fails after an Installed result, retain that exact newly visible
        // immutable set separately so the error terminal can transfer it to
        // operation-relative unreachable-residue custody.  Never increment a
        // carrier count without the matching installed/reused/residue state.
        let tally_failure = |sink: &mut Self| {
            if admission.outcome() == FsPackAdmissionOutcomeV1::Installed {
                sink.retain_pending_installed_residue_v1(admission.installed_residue_bytes_v1())?;
            }
            Err(CoreError::IntegerOverflow)
        };
        #[cfg(test)]
        if admission.outcome() == FsPackAdmissionOutcomeV1::Installed
            && self
                .control
                .borrow_mut()
                .inject_post_admission_carrier_tally_overflow()
        {
            return tally_failure(self);
        }
        let next_carrier_count = match self.carrier_count.checked_add(1) {
            Some(next) => next,
            None => return tally_failure(self),
        };
        let (next_installed, next_reused, next_residue) = match admission.outcome() {
            FsPackAdmissionOutcomeV1::Installed => {
                let next_installed = match self.carriers_installed.checked_add(1) {
                    Some(next) => next,
                    None => return tally_failure(self),
                };
                let next_residue = match self
                    .installed_residue_bytes
                    .checked_add(admission.installed_residue_bytes_v1())
                {
                    Some(next) => next,
                    None => return tally_failure(self),
                };
                (next_installed, self.carriers_reused, next_residue)
            }
            FsPackAdmissionOutcomeV1::ExistingComplete => {
                let next_reused = match self.carriers_reused.checked_add(1) {
                    Some(next) => next,
                    None => return tally_failure(self),
                };
                (
                    self.carriers_installed,
                    next_reused,
                    self.installed_residue_bytes,
                )
            }
        };
        self.carrier_count = next_carrier_count;
        self.carriers_installed = next_installed;
        self.carriers_reused = next_reused;
        self.installed_residue_bytes = next_residue;
        self.last_admission = Some((sealed, admission.outcome()));
        Ok(())
    }

    fn retain_pending_installed_residue_v1(&mut self, residue_bytes: u64) -> CoreResult<()> {
        // A terminal tally failure stops the sink before another admission can
        // occur.  Treat a second pending value as an internal sequencing
        // violation rather than overwriting the first exact custody record.
        if self.pending_installed_residue_bytes.is_some() {
            return Err(CoreError::PackInvalid);
        }
        self.pending_installed_residue_bytes = Some(residue_bytes);
        Ok(())
    }

    fn accumulate_carrier_counters_v1(
        &mut self,
        carrier_counters: OperationCountersV1,
    ) -> CoreResult<()> {
        // The general counter accumulator performs checked field-by-field
        // addition. Stage the merge so a late overflow cannot leave this
        // operation with a partially transferred carrier observation.
        let mut checked = self.storage_counters;
        #[cfg(test)]
        if self
            .control
            .borrow_mut()
            .inject_carrier_counter_accumulation_overflow()
        {
            checked.carrier_bytes_total = u64::MAX;
        }
        checked.accumulate(carrier_counters)?;
        self.storage_counters = checked;
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
            let mut shared_control = SharedOperationControlV1::new(self.control);
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
        // Stage both the normally tallied and post-admission-pending values.
        // If a late checked addition fails, neither source value is cleared
        // and no partial direct-residue observation becomes visible.
        let mut checked = self.storage_counters;
        if self.installed_residue_bytes != 0 {
            checked.record_unreachable_installed_residue(self.installed_residue_bytes)?;
        }
        if let Some(residue_bytes) = self.pending_installed_residue_bytes {
            checked.record_unreachable_installed_residue(residue_bytes)?;
        }
        self.storage_counters = checked;
        // Transfer this direct observation exactly once. Keep each value live
        // until the complete checked transaction succeeds, so terminal panic
        // recovery may call this method again without double-counting or
        // erasing any installed immutable custody.
        self.installed_residue_bytes = 0;
        self.pending_installed_residue_bytes = None;
        Ok(())
    }

    pub(crate) fn take_storage_counters(&mut self) -> OperationCountersV1 {
        core::mem::take(&mut self.storage_counters)
    }

    pub(crate) fn take_first_fscas_error(&mut self) -> Option<FsCasErrorV1> {
        self.first_fscas_error.take()
    }

    pub(crate) fn take_first_core_error(&mut self) -> Option<CoreError> {
        self.first_core_error.take()
    }

    pub(crate) fn cleanup_private_pack_controlled_v1(&mut self) -> Result<(), FsCasErrorV1> {
        let mut shared_control = SharedOperationControlV1::new(self.control);
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
        #[cfg(test)]
        if self
            .control
            .borrow_mut()
            .inject_global_seen_counter_accumulation_overflow()
        {
            self.storage_counters.global_seen_lookups = 41;
            self.storage_counters.global_seen_probes = 43;
            self.storage_counters.global_seen_metadata_bytes_read = 47;
            self.storage_counters.global_seen_metadata_read_calls = 53;
            self.storage_counters.global_seen_metadata_bytes_written = u64::MAX;
            self.storage_counters.global_seen_maximum_probe = 59;
            self.storage_counters.global_seen_entries = 61;
            self.storage_counters.global_seen_table_bytes = 67;
        }
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
            let mut control = SharedOperationControlV1::new(self.control);
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
            #[cfg(test)]
            if self
                .control
                .borrow_mut()
                .inject_pack_object_disposition_overflow(false)
            {
                self.storage_counters
                    .saturate_pack_object_disposition_for_test_v1(current.kind, false);
            }
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
        if let Err(error) = self.metadata.push(PackIndexEntryV1::from_validated_parts(
            expected_id,
            current.record_offset,
            u32::try_from(current.complete_len).map_err(|_| CoreError::IntegerOverflow)?,
            checksum,
        )) {
            return Err(self.promote_metadata_spool_error_v1(error));
        }
        let carrier_ordinal = self.carrier_count;
        let mut shared_control = SharedOperationControlV1::new(self.control);
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
        #[cfg(test)]
        if self
            .control
            .borrow_mut()
            .inject_pack_object_disposition_overflow(true)
        {
            self.storage_counters
                .saturate_pack_object_disposition_for_test_v1(current.kind, true);
        }
        self.storage_counters
            .record_pack_object_disposition(current.kind, true)?;
        Ok(ObjectDispositionV1::Created)
    }

    fn compare_objects(&mut self, left: u64, right: u64, len: u64) -> CoreResult<bool> {
        let mut offset = 0_u64;
        while offset < len {
            self.poll_control_v1()?;
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
            #[cfg(test)]
            if self
                .control
                .borrow_mut()
                .inject_same_carrier_comparison_observation_overflow()
            {
                self.direct_read_bytes = 71;
                self.direct_read_calls = u64::MAX;
            }
            let read_bytes = u64::try_from(take)
                .map_err(|_| CoreError::IntegerOverflow)?
                .checked_mul(2)
                .ok_or(CoreError::IntegerOverflow)?;
            accumulate_direct_read_observation_v1(
                &mut self.direct_read_bytes,
                &mut self.direct_read_calls,
                read_bytes,
                2,
            )?;
            if self.left[..take] != self.right[..take] {
                return Ok(false);
            }
            offset = offset
                .checked_add(take as u64)
                .ok_or(CoreError::IntegerOverflow)?;
        }
        self.poll_control_v1()?;
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
            let occupied_len = match {
                let mut control = SharedOperationControlV1::new(self.control);
                self.occupied
                    .occupied_len_typed_controlled_v1(id, &mut control)
            } {
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
                self.poll_control_v1()?;
                let take = usize::try_from((len - offset).min(COMPARISON_WINDOW_BYTES as u64))
                    .map_err(|_| CoreError::IntegerOverflow)?;
                let occupied_read = {
                    let mut control = SharedOperationControlV1::new(self.control);
                    self.occupied.read_occupied_exact_at_typed_controlled_v1(
                        id,
                        offset,
                        &mut self.left[..take],
                        &mut control,
                    )
                };
                if let Err(error) = occupied_read {
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
                accumulate_direct_read_observation_v1(
                    &mut self.direct_read_bytes,
                    &mut self.direct_read_calls,
                    u64::try_from(take).map_err(|_| CoreError::IntegerOverflow)?,
                    1,
                )?;
                if self.left[..take] != self.right[..take] {
                    return Ok(false);
                }
                offset = offset
                    .checked_add(take as u64)
                    .ok_or(CoreError::IntegerOverflow)?;
            }
            self.poll_control_v1()?;
            Ok(true)
        })();
        // The direct occupied-read delta is required even when the comparison
        // body has already failed.  It is, however, post-body observation:
        // a later read-observation or counter failure cannot replace the
        // first comparison cause.  In particular, do not route such a later
        // FsCas error through `map_occupied_fscas_error` before the ordered
        // terminal decision below; doing so would make the lifecycle prefer
        // it over the earlier Core error side channel.
        let post_body_observation = (|| {
            let after = self
                .occupied
                .direct_storage_read_observation_typed_v1()
                .map_err(OccupiedComparisonTerminalV1::PostFsCas)?;
            let bytes_read =
                after
                    .0
                    .checked_sub(before.0)
                    .ok_or(OccupiedComparisonTerminalV1::PostCore(
                        CoreError::IntegerOverflow,
                    ))?;
            let read_calls =
                after
                    .1
                    .checked_sub(before.1)
                    .ok_or(OccupiedComparisonTerminalV1::PostCore(
                        CoreError::IntegerOverflow,
                    ))?;
            self.storage_counters
                .record_fscas_read(bytes_read, read_calls)
                .map_err(OccupiedComparisonTerminalV1::PostCore)
        })();
        match finish_occupied_comparison_v1(result, post_body_observation) {
            Ok(equal) => Ok(equal),
            Err(OccupiedComparisonTerminalV1::Body(error)) => {
                self.first_core_error.get_or_insert(error);
                Err(error)
            }
            Err(OccupiedComparisonTerminalV1::PostFsCas(error)) => {
                Err(self.map_occupied_fscas_error(error))
            }
            Err(OccupiedComparisonTerminalV1::PostCore(error)) => Err(error),
        }
    }

    fn append_pack(&mut self, bytes: &[u8]) -> CoreResult<()> {
        let result = {
            let mut control = SharedOperationControlV1::new(self.control);
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
            let mut shared_control = SharedOperationControlV1::new(self.control);
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
            let mut control = SharedOperationControlV1::new(self.control);
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
            let mut shared_control = SharedOperationControlV1::new(self.control);
            if let Err(error) = self
                .metadata
                .sort_by_key_controlled(&mut shared_control, &mut self.storage_counters)
            {
                return Err(self.promote_metadata_spool_error_v1(error));
            }
        }
        if let Err(error) = self.metadata.rewind() {
            return Err(self.promote_metadata_spool_error_v1(error));
        }
        let mut emitted = 0_u32;
        let mut previous = None;
        loop {
            self.poll_control_v1()?;
            let entry = match self.metadata.next() {
                Ok(Some(entry)) => entry,
                Ok(None) => break,
                Err(error) => return Err(self.promote_metadata_spool_error_v1(error)),
            };
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
            first_core_error: None,
        };
        #[cfg(test)]
        if self
            .control
            .borrow_mut()
            .inject_counted_pack_read_observation_overflow()
        {
            counted_pack.bytes_read = 71;
            counted_pack.read_calls = u64::MAX;
        }
        let digest_result = {
            let mut control = SharedOperationControlV1::new(self.control);
            hash_port_range_controlled_v1(
                &mut counted_pack,
                0,
                checksum_len,
                TAG_PACK,
                self.left,
                counters,
                &mut control,
            )
        };
        let next_direct_read_bytes = self
            .direct_read_bytes
            .checked_add(counted_pack.bytes_read)
            .ok_or(CoreError::IntegerOverflow)?;
        let next_direct_read_calls = self
            .direct_read_calls
            .checked_add(counted_pack.read_calls)
            .ok_or(CoreError::IntegerOverflow)?;
        self.direct_read_bytes = next_direct_read_bytes;
        self.direct_read_calls = next_direct_read_calls;
        let counted_core_error = counted_pack.first_core_error;
        if let Some(error) = counted_core_error {
            self.first_core_error.get_or_insert(error);
        }
        let digest = match digest_result {
            Ok(digest) => digest,
            Err(error) => {
                if let Some(storage_error) = self.pack.take_first_error_typed_v1() {
                    self.first_fscas_error.get_or_insert(storage_error);
                    return Err(map_fscas_operation(storage_error));
                }
                if let Some(core_error) = counted_core_error {
                    return Err(core_error);
                }
                return Err(error);
            }
        };
        self.append_pack(&digest)?;
        let id = PackIdV1::from_digest(digest);
        let seal_result = {
            let mut control = SharedOperationControlV1::new(self.control);
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

    fn promote_metadata_spool_error_v1(&mut self, error: PackPortErrorV1) -> CoreError {
        if let Some(storage_error) = self.metadata.take_storage_error_typed_v1() {
            self.first_fscas_error.get_or_insert(storage_error);
            map_fscas_operation(storage_error)
        } else {
            map_spool(error)
        }
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
            .map_err(|error| self.retain_prepared_sink_core_error_v1(error))
    }

    fn begin_object(
        &mut self,
        kind: PhysicalObjectKindV1,
        complete_len: u64,
    ) -> Result<(), PreparedSinkErrorV1> {
        self.begin_object_inner(kind, complete_len)
            .map_err(|error| self.retain_prepared_sink_core_error_v1(error))
    }

    fn write_private(&mut self, bytes: &[u8]) -> Result<(), PreparedSinkErrorV1> {
        self.write_inner(bytes)
            .map_err(|error| self.retain_prepared_sink_core_error_v1(error))
    }

    fn finish_object(
        &mut self,
        expected_id: TypedPhysicalObjectIdV1,
    ) -> Result<ObjectDispositionV1, PreparedSinkErrorV1> {
        self.finish_object_inner(expected_id)
            .map_err(|error| self.retain_prepared_sink_core_error_v1(error))
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
            .map_err(|error| self.retain_tree_sink_core_error_v1(error))
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
            .map_err(|error| self.retain_tree_sink_core_error_v1(error))
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

fn accumulate_direct_read_observation_v1(
    bytes_read: &mut u64,
    read_calls: &mut u64,
    additional_bytes: u64,
    additional_calls: u64,
) -> CoreResult<()> {
    let next_bytes = bytes_read
        .checked_add(additional_bytes)
        .ok_or(CoreError::IntegerOverflow)?;
    let next_calls = read_calls
        .checked_add(additional_calls)
        .ok_or(CoreError::IntegerOverflow)?;
    *bytes_read = next_bytes;
    *read_calls = next_calls;
    Ok(())
}

/// Ordered terminal result for an occupied-object comparison.  The read
/// observation after the comparison is mandatory direct attribution, but it
/// occurs after the comparison body and therefore cannot replace its first
/// typed outcome.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum OccupiedComparisonTerminalV1 {
    Body(CoreError),
    PostFsCas(FsCasErrorV1),
    PostCore(CoreError),
}

fn finish_occupied_comparison_v1(
    body: CoreResult<bool>,
    post_body_observation: Result<(), OccupiedComparisonTerminalV1>,
) -> Result<bool, OccupiedComparisonTerminalV1> {
    match body {
        Ok(equal) => post_body_observation.map(|()| equal),
        Err(error) => {
            // The caller has already completed the post-body observation by
            // the time it invokes this helper.  Deliberately discard a
            // non-dominant later observation error while preserving the
            // comparison's chronological first cause.
            let _ = post_body_observation;
            Err(OccupiedComparisonTerminalV1::Body(error))
        }
    }
}

/// Combines an error while starting a replacement carrier with the explicit
/// cleanup of its newly opened private pack.  The caller must retain the
/// result in its FsCas side channel because lifecycle terminalization gives
/// that exact typed custody result precedence over the generic sink mapping.
fn next_private_pack_cleanup_terminal_v1(first: CoreError, cleanup: FsCasErrorV1) -> FsCasErrorV1 {
    FsCasErrorV1::Core(first).dominated_by_v1(cleanup)
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
        | FsCasErrorV1::SynchronizationPoisoned
        | FsCasErrorV1::CrossOwner
        | FsCasErrorV1::WrongOperationKind
        | FsCasErrorV1::CleanupFailed(_)
        | FsCasErrorV1::InvalidationFailed
        | FsCasErrorV1::MalformedOccupant
        | FsCasErrorV1::MissingOccupant
        | FsCasErrorV1::UnequalOccupant
        | FsCasErrorV1::Filesystem(_)
        | FsCasErrorV1::Io
        | FsCasErrorV1::Integrity
        | FsCasErrorV1::Collision
        | FsCasErrorV1::TerminalFailure { .. } => CoreError::SinkRefused,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cas::{FsCasCleanupTargetV1, FsCasFailureCauseV1};

    struct FillingReadPortV1 {
        bytes: [u8; 4],
        reads: u64,
    }

    impl PackReadPortV1 for FillingReadPortV1 {
        fn resident_memory_bound_bytes(&self) -> CoreResult<u64> {
            Ok(0)
        }

        fn len(&mut self) -> Result<u64, PackPortErrorV1> {
            Ok(self.bytes.len() as u64)
        }

        fn read_exact_at(
            &mut self,
            offset: u64,
            destination: &mut [u8],
        ) -> Result<(), PackPortErrorV1> {
            let start = usize::try_from(offset).map_err(|_| PackPortErrorV1::Failure)?;
            let end = start
                .checked_add(destination.len())
                .ok_or(PackPortErrorV1::Failure)?;
            let source = self.bytes.get(start..end).ok_or(PackPortErrorV1::Failure)?;
            destination.copy_from_slice(source);
            self.reads += 1;
            Ok(())
        }
    }

    #[test]
    fn counted_pack_read_late_overflow_keeps_the_observation_tuple_atomic() {
        let mut port = FillingReadPortV1 {
            bytes: [0x11, 0x22, 0x33, 0x44],
            reads: 0,
        };
        let mut counted = CountedPackReadV1 {
            pack: &mut port,
            bytes_read: 71,
            read_calls: u64::MAX,
            first_core_error: None,
        };
        let mut destination = [0_u8; 4];

        assert_eq!(
            counted.read_exact_at(0, &mut destination),
            Err(PackPortErrorV1::Failure)
        );
        assert_eq!(destination, [0x11, 0x22, 0x33, 0x44]);
        assert_eq!((counted.bytes_read, counted.read_calls), (71, u64::MAX));
        assert_eq!(counted.first_core_error, Some(CoreError::IntegerOverflow));
        assert_eq!(port.reads, 1);
    }

    #[test]
    fn direct_read_observation_commits_both_fields_or_neither() {
        let mut bytes_read = 41;
        let mut read_calls = 43;

        accumulate_direct_read_observation_v1(&mut bytes_read, &mut read_calls, 47, 53).unwrap();
        assert_eq!((bytes_read, read_calls), (88, 96));

        bytes_read = 71;
        read_calls = u64::MAX;
        assert_eq!(
            accumulate_direct_read_observation_v1(&mut bytes_read, &mut read_calls, 59, 1),
            Err(CoreError::IntegerOverflow)
        );
        assert_eq!((bytes_read, read_calls), (71, u64::MAX));
    }

    #[test]
    fn occupied_comparison_keeps_a_body_error_when_post_body_attribution_fails() {
        assert_eq!(
            finish_occupied_comparison_v1(
                Err(CoreError::IdMismatch),
                Err(OccupiedComparisonTerminalV1::PostFsCas(
                    FsCasErrorV1::Invalidated,
                )),
            ),
            Err(OccupiedComparisonTerminalV1::Body(CoreError::IdMismatch))
        );
        assert_eq!(
            finish_occupied_comparison_v1(
                Err(CoreError::IdMismatch),
                Err(OccupiedComparisonTerminalV1::PostCore(
                    CoreError::IntegerOverflow,
                )),
            ),
            Err(OccupiedComparisonTerminalV1::Body(CoreError::IdMismatch))
        );
    }

    #[test]
    fn occupied_comparison_returns_post_body_attribution_failure_after_a_successful_body() {
        assert_eq!(
            finish_occupied_comparison_v1(
                Ok(true),
                Err(OccupiedComparisonTerminalV1::PostFsCas(
                    FsCasErrorV1::Invalidated,
                )),
            ),
            Err(OccupiedComparisonTerminalV1::PostFsCas(
                FsCasErrorV1::Invalidated,
            ))
        );
        assert_eq!(finish_occupied_comparison_v1(Ok(false), Ok(())), Ok(false));
    }

    #[test]
    fn next_private_pack_cleanup_preserves_the_start_failure() {
        for (first, cleanup, dominant) in [
            (
                CoreError::ResourceRefused,
                FsCasErrorV1::CleanupFailed(FsCasCleanupTargetV1::PrivatePack),
                FsCasFailureCauseV1::CleanupFailed(FsCasCleanupTargetV1::PrivatePack),
            ),
            (
                CoreError::IntegerOverflow,
                FsCasErrorV1::InvalidationFailed,
                FsCasFailureCauseV1::InvalidationFailed,
            ),
        ] {
            assert_eq!(
                next_private_pack_cleanup_terminal_v1(first, cleanup),
                FsCasErrorV1::TerminalFailure {
                    first: FsCasFailureCauseV1::Core(first),
                    dominant,
                }
            );
        }
    }
}
