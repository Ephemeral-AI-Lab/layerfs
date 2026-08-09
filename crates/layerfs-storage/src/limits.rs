//! Fixed L1 memory ledger and proof counters.

use core::sync::atomic::{AtomicU64, Ordering};

use crate::{CoreError, CoreResult};

pub const BASE_LEDGER_BYTES: u64 = 8_388_608;
pub const OPERATION_SLOT_BYTES: u64 = 4_194_304;
pub const MEMORY_PROFILE_32_MIB: u64 = 32 * 1_024 * 1_024;
pub const MEMORY_PROFILE_48_MIB: u64 = 48 * 1_024 * 1_024;
pub const MEMORY_PROFILE_72_MIB: u64 = 72 * 1_024 * 1_024;
const MEMORY_COMPONENT_COUNT: usize = 9;

pub const fn admitted_slots_for_budget(budget_bytes: u64) -> u64 {
    if budget_bytes < BASE_LEDGER_BYTES {
        0
    } else {
        (budget_bytes - BASE_LEDGER_BYTES) / OPERATION_SLOT_BYTES
    }
}

#[derive(Debug)]
pub struct ResourceLedgerV1 {
    capacity_slots: u64,
    admitted_slots: AtomicU64,
    high_water_slots: AtomicU64,
    live_planned_bytes: AtomicU64,
    high_water_planned_bytes: AtomicU64,
}

impl ResourceLedgerV1 {
    pub const fn new(budget_bytes: u64) -> Self {
        Self {
            capacity_slots: admitted_slots_for_budget(budget_bytes),
            admitted_slots: AtomicU64::new(0),
            high_water_slots: AtomicU64::new(0),
            live_planned_bytes: AtomicU64::new(0),
            high_water_planned_bytes: AtomicU64::new(0),
        }
    }

    pub const fn capacity_slots(&self) -> u64 {
        self.capacity_slots
    }

    pub fn admitted_slots(&self) -> u64 {
        self.admitted_slots.load(Ordering::Acquire)
    }

    pub fn high_water_bytes(&self) -> u64 {
        BASE_LEDGER_BYTES + self.high_water_slots.load(Ordering::Acquire) * OPERATION_SLOT_BYTES
    }

    /// High-water of explicitly named live operation buffers, excluding the
    /// frozen 8 MiB handle reserve. This is evidence about actual working
    /// sets; [`Self::high_water_bytes`] remains the conservative slot bound.
    pub fn planned_high_water_bytes(&self) -> u64 {
        BASE_LEDGER_BYTES + self.high_water_planned_bytes.load(Ordering::Acquire)
    }

    pub fn reserve_operation(&self) -> CoreResult<OperationReservationV1<'_>> {
        self.reserve_operation_with_plan(OperationMemoryPlanV1::empty())
    }

    pub fn reserve_operation_with_plan(
        &self,
        plan: OperationMemoryPlanV1,
    ) -> CoreResult<OperationReservationV1<'_>> {
        if plan.total_bytes > OPERATION_SLOT_BYTES {
            return Err(CoreError::ResourceRefused);
        }
        let mut current = self.admitted_slots.load(Ordering::Acquire);
        loop {
            if current >= self.capacity_slots {
                return Err(CoreError::ResourceRefused);
            }
            let next = current.checked_add(1).ok_or(CoreError::IntegerOverflow)?;
            match self.admitted_slots.compare_exchange_weak(
                current,
                next,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => {
                    let mut live_before = self.live_planned_bytes.load(Ordering::Acquire);
                    let live = loop {
                        let Some(live_after) = live_before.checked_add(plan.total_bytes) else {
                            self.admitted_slots.fetch_sub(1, Ordering::AcqRel);
                            return Err(CoreError::IntegerOverflow);
                        };
                        match self.live_planned_bytes.compare_exchange_weak(
                            live_before,
                            live_after,
                            Ordering::AcqRel,
                            Ordering::Acquire,
                        ) {
                            Ok(_) => break live_after,
                            Err(observed) => live_before = observed,
                        }
                    };
                    self.high_water_slots.fetch_max(next, Ordering::AcqRel);
                    self.high_water_planned_bytes
                        .fetch_max(live, Ordering::AcqRel);
                    return Ok(OperationReservationV1 {
                        ledger: self,
                        released: false,
                        planned_bytes: plan.total_bytes,
                        plan,
                    });
                }
                Err(observed) => current = observed,
            }
        }
    }
}

#[derive(Debug)]
pub struct OperationReservationV1<'ledger> {
    ledger: &'ledger ResourceLedgerV1,
    released: bool,
    planned_bytes: u64,
    plan: OperationMemoryPlanV1,
}

impl OperationReservationV1<'_> {
    /// Prove that an already-admitted operation owns every named buffer a
    /// lower layer will borrow. This performs no admission and cannot consume
    /// another slot.
    pub(crate) fn require(&self, required: OperationMemoryPlanV1) -> CoreResult<()> {
        if self.plan.covers(required) {
            Ok(())
        } else {
            Err(CoreError::ResourceRefused)
        }
    }
}

impl Drop for OperationReservationV1<'_> {
    fn drop(&mut self) {
        if !self.released {
            let previous = self.ledger.admitted_slots.fetch_sub(1, Ordering::AcqRel);
            debug_assert!(previous > 0);
            let previous_bytes = self
                .ledger
                .live_planned_bytes
                .fetch_sub(self.planned_bytes, Ordering::AcqRel);
            debug_assert!(previous_bytes >= self.planned_bytes);
            self.released = true;
        }
    }
}

/// Every LayerFS-owned or caller-provided operation buffer has one stable
/// accounting domain. A plan rejects duplicate domains, which makes omitted
/// and double-charged buffers visible in code review and conformance tests.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum MemoryComponentV1 {
    CdcRing = 0,
    SourceWindow = 1,
    ComparisonWindow = 2,
    ObjectScratch = 3,
    PageSummaries = 4,
    TraversalState = 5,
    EvidenceWindow = 6,
    MetadataWindow = 7,
    HashState = 8,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct OperationMemoryPlanV1 {
    components: u16,
    total_bytes: u64,
    component_bytes: [u64; MEMORY_COMPONENT_COUNT],
}

impl OperationMemoryPlanV1 {
    pub const fn empty() -> Self {
        Self {
            components: 0,
            total_bytes: 0,
            component_bytes: [0; MEMORY_COMPONENT_COUNT],
        }
    }

    pub fn charge(mut self, component: MemoryComponentV1, bytes: u64) -> CoreResult<Self> {
        let bit = 1_u16
            .checked_shl(component as u32)
            .ok_or(CoreError::IntegerOverflow)?;
        if self.components & bit != 0 {
            return Err(CoreError::ResourceRefused);
        }
        self.total_bytes = self
            .total_bytes
            .checked_add(bytes)
            .ok_or(CoreError::IntegerOverflow)?;
        if self.total_bytes > OPERATION_SLOT_BYTES {
            return Err(CoreError::ResourceRefused);
        }
        self.components |= bit;
        self.component_bytes[component as usize] = bytes;
        Ok(self)
    }

    pub const fn total_bytes(self) -> u64 {
        self.total_bytes
    }

    pub const fn contains(self, component: MemoryComponentV1) -> bool {
        self.components & (1_u16 << component as u8) != 0
    }

    const fn covers(self, required: Self) -> bool {
        let mut index = 0_usize;
        while index < MEMORY_COMPONENT_COUNT {
            if self.component_bytes[index] < required.component_bytes[index] {
                return false;
            }
            index += 1;
        }
        true
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct OperationCountersV1 {
    pub bytes_read: u64,
    pub source_read_calls: u64,
    pub source_bytes_read: u64,
    pub bytes_written: u64,
    pub bytes_copied: u64,
    pub bytes_boundary_inspected: u64,
    pub ring_fills: u64,
    pub ring_wrap_spans: u64,
    pub cdc_scan_calls: u64,
    pub cdc_scan_bytes: u64,
    pub seqcdc_comparisons: u64,
    pub seqcdc_equal_absorptions: u64,
    pub seqcdc_opposing_slopes: u64,
    pub seqcdc_jumps: u64,
    pub seqcdc_jump_bytes: u64,
    pub logical_hash_bytes: u64,
    pub logical_hash_update_calls: u64,
    pub physical_hash_bytes: u64,
    pub physical_hash_update_calls: u64,
    pub bytes_structurally_reused: u64,
    pub logical_chunks_created: u64,
    pub logical_chunks_reused: u64,
    pub physical_objects_created: u64,
    pub physical_objects_reused: u64,
    pub tree_nodes_created: u64,
    pub tree_nodes_reused: u64,
    pub pack_entries: u64,
    pub pack_bytes: u64,
    pub pack_allocated_blocks: u64,
    pub temporary_preparation_bytes: u64,
    pub fscas_bytes_read: u64,
    pub fscas_read_calls: u64,
    pub fscas_bytes_written: u64,
    pub fscas_catalog_operations: u64,
    pub incumbent_comparison_bytes: u64,
    pub incumbent_comparison_windows: u64,
    pub closure_fences: u64,
    pub unreachable_installed_residue_bytes: u64,
    pub update_base_payload_bytes: u64,
    pub update_inserted_bytes: u64,
    pub update_reference_metadata_records: u64,
    pub update_reference_metadata_bytes: u64,
    pub update_resynchronization_bytes: u64,
    pub anchor_attempts: u64,
    pub exact_rejoin_bytes: u64,
    pub rejoin_successes: u64,
    pub rejoin_failures: u64,
    pub update_failures: u64,
    pub fallback_attempts: u64,
    pub retries_or_redispatches: u64,
    pub provider_switches: u64,
    pub cdc_switches: u64,
    pub publication_dispatches: u64,
    pub file_sync_calls: u64,
    pub directory_sync_calls: u64,
    pub memory_high_water: u64,
    pub allocator_high_water: u64,
    pub rss_high_water: u64,
    pub pss_high_water: u64,
    pub page_cache_observed_bytes: u64,
    pub open_files_high_water: u64,
}

impl OperationCountersV1 {
    pub(crate) fn record_source_bytes_read(&mut self, bytes: u64) -> CoreResult<()> {
        let mut checked = *self;
        checked.add(CounterFieldV1::SourceBytesRead, bytes)?;
        checked.add(CounterFieldV1::BytesRead, bytes)?;
        *self = checked;
        Ok(())
    }

    pub(crate) fn add_cdc_stream(
        &mut self,
        counters: crate::cdc::CdcStreamCountersV1,
    ) -> CoreResult<()> {
        self.add(CounterFieldV1::RingFills, counters.ring_fills)?;
        self.add(CounterFieldV1::RingWrapSpans, counters.ring_wrap_spans)?;
        self.add(CounterFieldV1::CdcScanCalls, counters.scan_calls)?;
        self.add(CounterFieldV1::CdcScanBytes, counters.scan_bytes)?;
        self.add(
            CounterFieldV1::BytesBoundaryInspected,
            counters.boundary_inspected_bytes,
        )
    }

    pub fn add_seqcdc(&mut self, counters: crate::cdc::SeqCdcCountersV1) -> CoreResult<()> {
        self.add(CounterFieldV1::SeqCdcComparisons, counters.comparisons)?;
        self.add(
            CounterFieldV1::SeqCdcEqualAbsorptions,
            counters.equal_absorptions,
        )?;
        self.add(
            CounterFieldV1::SeqCdcOpposingSlopes,
            counters.opposing_slopes,
        )?;
        self.add(CounterFieldV1::SeqCdcJumps, counters.jumps)?;
        self.add(CounterFieldV1::SeqCdcJumpBytes, counters.jump_bytes)
    }

    pub fn record_pack_storage(
        &mut self,
        allocated_blocks: u64,
        temporary_bytes: u64,
    ) -> CoreResult<()> {
        self.add(CounterFieldV1::PackAllocatedBlocks, allocated_blocks)?;
        self.add(CounterFieldV1::TemporaryPreparationBytes, temporary_bytes)
    }

    pub fn record_fscas_read(&mut self, bytes: u64, calls: u64) -> CoreResult<()> {
        let mut checked = *self;
        checked.add(CounterFieldV1::FsCasBytesRead, bytes)?;
        checked.add(CounterFieldV1::FsCasReadCalls, calls)?;
        *self = checked;
        Ok(())
    }

    pub fn record_fscas_write(&mut self, bytes: u64) -> CoreResult<()> {
        self.add(CounterFieldV1::FsCasBytesWritten, bytes)
    }

    pub fn record_fscas_catalog_operation(&mut self) -> CoreResult<()> {
        self.add(CounterFieldV1::FsCasCatalogOperations, 1)
    }

    pub fn record_incumbent_comparison(&mut self, bytes: u64, windows: u64) -> CoreResult<()> {
        self.add(CounterFieldV1::IncumbentComparisonBytes, bytes)?;
        self.add(CounterFieldV1::IncumbentComparisonWindows, windows)
    }

    pub fn record_closure_fence(&mut self) -> CoreResult<()> {
        self.add(CounterFieldV1::ClosureFences, 1)
    }

    pub fn record_unreachable_installed_residue(&mut self, bytes: u64) -> CoreResult<()> {
        self.add(CounterFieldV1::UnreachableInstalledResidueBytes, bytes)
    }

    pub fn observe_open_files(&mut self, open_files: u64) {
        self.open_files_high_water = self.open_files_high_water.max(open_files);
    }

    pub fn record_update_base_payload(&mut self, bytes: u64) -> CoreResult<()> {
        self.add(CounterFieldV1::UpdateBasePayloadBytes, bytes)
    }

    pub fn record_update_inserted(&mut self, bytes: u64) -> CoreResult<()> {
        self.add(CounterFieldV1::UpdateInsertedBytes, bytes)
    }

    pub fn record_update_reference_metadata(&mut self, records: u64, bytes: u64) -> CoreResult<()> {
        self.add(CounterFieldV1::UpdateReferenceMetadataRecords, records)?;
        self.add(CounterFieldV1::UpdateReferenceMetadataBytes, bytes)
    }

    pub fn record_exact_rejoin(&mut self, bytes: u64, equal: bool) -> CoreResult<()> {
        self.add(CounterFieldV1::ExactRejoinBytes, bytes)?;
        self.add(
            if equal {
                CounterFieldV1::RejoinSuccesses
            } else {
                CounterFieldV1::RejoinFailures
            },
            1,
        )
    }

    pub fn record_fallback_attempt(&mut self) -> CoreResult<()> {
        self.add(CounterFieldV1::FallbackAttempts, 1)
    }

    pub fn record_retry_or_redispatch(&mut self) -> CoreResult<()> {
        self.add(CounterFieldV1::RetriesOrRedispatches, 1)
    }

    pub fn record_publication_dispatch(&mut self) -> CoreResult<()> {
        self.add(CounterFieldV1::PublicationDispatches, 1)
    }

    pub fn record_provider_switch(&mut self) -> CoreResult<()> {
        self.add(CounterFieldV1::ProviderSwitches, 1)
    }

    pub fn record_cdc_switch(&mut self) -> CoreResult<()> {
        self.add(CounterFieldV1::CdcSwitches, 1)
    }

    pub fn record_file_sync(&mut self) -> CoreResult<()> {
        self.add(CounterFieldV1::FileSyncCalls, 1)
    }

    pub fn record_directory_sync(&mut self) -> CoreResult<()> {
        self.add(CounterFieldV1::DirectorySyncCalls, 1)
    }

    pub const fn has_zero_forbidden_work(&self) -> bool {
        self.fallback_attempts == 0
            && self.retries_or_redispatches == 0
            && self.provider_switches == 0
            && self.cdc_switches == 0
            && self.publication_dispatches == 0
            && self.file_sync_calls == 0
            && self.directory_sync_calls == 0
    }

    pub(crate) fn add(&mut self, field: CounterFieldV1, amount: u64) -> CoreResult<()> {
        let target = match field {
            CounterFieldV1::BytesRead => &mut self.bytes_read,
            CounterFieldV1::SourceReadCalls => &mut self.source_read_calls,
            CounterFieldV1::SourceBytesRead => &mut self.source_bytes_read,
            CounterFieldV1::BytesWritten => &mut self.bytes_written,
            CounterFieldV1::BytesCopied => &mut self.bytes_copied,
            CounterFieldV1::BytesBoundaryInspected => &mut self.bytes_boundary_inspected,
            CounterFieldV1::RingFills => &mut self.ring_fills,
            CounterFieldV1::RingWrapSpans => &mut self.ring_wrap_spans,
            CounterFieldV1::CdcScanCalls => &mut self.cdc_scan_calls,
            CounterFieldV1::CdcScanBytes => &mut self.cdc_scan_bytes,
            CounterFieldV1::SeqCdcComparisons => &mut self.seqcdc_comparisons,
            CounterFieldV1::SeqCdcEqualAbsorptions => &mut self.seqcdc_equal_absorptions,
            CounterFieldV1::SeqCdcOpposingSlopes => &mut self.seqcdc_opposing_slopes,
            CounterFieldV1::SeqCdcJumps => &mut self.seqcdc_jumps,
            CounterFieldV1::SeqCdcJumpBytes => &mut self.seqcdc_jump_bytes,
            CounterFieldV1::LogicalHashBytes => &mut self.logical_hash_bytes,
            CounterFieldV1::LogicalHashUpdateCalls => &mut self.logical_hash_update_calls,
            CounterFieldV1::PhysicalHashBytes => &mut self.physical_hash_bytes,
            CounterFieldV1::PhysicalHashUpdateCalls => &mut self.physical_hash_update_calls,
            CounterFieldV1::BytesStructurallyReused => &mut self.bytes_structurally_reused,
            CounterFieldV1::LogicalChunksCreated => &mut self.logical_chunks_created,
            CounterFieldV1::LogicalChunksReused => &mut self.logical_chunks_reused,
            CounterFieldV1::PhysicalObjectsCreated => &mut self.physical_objects_created,
            CounterFieldV1::PhysicalObjectsReused => &mut self.physical_objects_reused,
            CounterFieldV1::TreeNodesCreated => &mut self.tree_nodes_created,
            CounterFieldV1::TreeNodesReused => &mut self.tree_nodes_reused,
            CounterFieldV1::PackEntries => &mut self.pack_entries,
            CounterFieldV1::PackBytes => &mut self.pack_bytes,
            CounterFieldV1::PackAllocatedBlocks => &mut self.pack_allocated_blocks,
            CounterFieldV1::TemporaryPreparationBytes => &mut self.temporary_preparation_bytes,
            CounterFieldV1::FsCasBytesRead => &mut self.fscas_bytes_read,
            CounterFieldV1::FsCasReadCalls => &mut self.fscas_read_calls,
            CounterFieldV1::FsCasBytesWritten => &mut self.fscas_bytes_written,
            CounterFieldV1::FsCasCatalogOperations => &mut self.fscas_catalog_operations,
            CounterFieldV1::IncumbentComparisonBytes => &mut self.incumbent_comparison_bytes,
            CounterFieldV1::IncumbentComparisonWindows => &mut self.incumbent_comparison_windows,
            CounterFieldV1::ClosureFences => &mut self.closure_fences,
            CounterFieldV1::UnreachableInstalledResidueBytes => {
                &mut self.unreachable_installed_residue_bytes
            }
            CounterFieldV1::UpdateBasePayloadBytes => &mut self.update_base_payload_bytes,
            CounterFieldV1::UpdateInsertedBytes => &mut self.update_inserted_bytes,
            CounterFieldV1::UpdateReferenceMetadataRecords => {
                &mut self.update_reference_metadata_records
            }
            CounterFieldV1::UpdateReferenceMetadataBytes => {
                &mut self.update_reference_metadata_bytes
            }
            CounterFieldV1::UpdateResynchronizationBytes => {
                &mut self.update_resynchronization_bytes
            }
            CounterFieldV1::AnchorAttempts => &mut self.anchor_attempts,
            CounterFieldV1::ExactRejoinBytes => &mut self.exact_rejoin_bytes,
            CounterFieldV1::RejoinSuccesses => &mut self.rejoin_successes,
            CounterFieldV1::RejoinFailures => &mut self.rejoin_failures,
            CounterFieldV1::UpdateFailures => &mut self.update_failures,
            CounterFieldV1::FallbackAttempts => &mut self.fallback_attempts,
            CounterFieldV1::RetriesOrRedispatches => &mut self.retries_or_redispatches,
            CounterFieldV1::ProviderSwitches => &mut self.provider_switches,
            CounterFieldV1::CdcSwitches => &mut self.cdc_switches,
            CounterFieldV1::PublicationDispatches => &mut self.publication_dispatches,
            CounterFieldV1::FileSyncCalls => &mut self.file_sync_calls,
            CounterFieldV1::DirectorySyncCalls => &mut self.directory_sync_calls,
        };
        *target = target
            .checked_add(amount)
            .ok_or(CoreError::IntegerOverflow)?;
        Ok(())
    }
}

#[derive(Clone, Copy)]
pub(crate) enum CounterFieldV1 {
    BytesRead,
    SourceReadCalls,
    SourceBytesRead,
    BytesWritten,
    BytesCopied,
    BytesBoundaryInspected,
    RingFills,
    RingWrapSpans,
    CdcScanCalls,
    CdcScanBytes,
    SeqCdcComparisons,
    SeqCdcEqualAbsorptions,
    SeqCdcOpposingSlopes,
    SeqCdcJumps,
    SeqCdcJumpBytes,
    LogicalHashBytes,
    LogicalHashUpdateCalls,
    PhysicalHashBytes,
    PhysicalHashUpdateCalls,
    BytesStructurallyReused,
    LogicalChunksCreated,
    LogicalChunksReused,
    PhysicalObjectsCreated,
    PhysicalObjectsReused,
    TreeNodesCreated,
    TreeNodesReused,
    PackEntries,
    PackBytes,
    PackAllocatedBlocks,
    TemporaryPreparationBytes,
    FsCasBytesRead,
    FsCasReadCalls,
    FsCasBytesWritten,
    FsCasCatalogOperations,
    IncumbentComparisonBytes,
    IncumbentComparisonWindows,
    ClosureFences,
    UnreachableInstalledResidueBytes,
    UpdateBasePayloadBytes,
    UpdateInsertedBytes,
    UpdateReferenceMetadataRecords,
    UpdateReferenceMetadataBytes,
    UpdateResynchronizationBytes,
    AnchorAttempts,
    ExactRejoinBytes,
    RejoinSuccesses,
    RejoinFailures,
    UpdateFailures,
    FallbackAttempts,
    RetriesOrRedispatches,
    ProviderSwitches,
    CdcSwitches,
    PublicationDispatches,
    FileSyncCalls,
    DirectorySyncCalls,
}
