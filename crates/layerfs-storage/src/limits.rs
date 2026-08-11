//! Fixed L1 memory ledger and proof counters.

use core::sync::atomic::{AtomicU64, Ordering};

use crate::format::PhysicalObjectKindV1;
use crate::{CoreError, CoreResult};

pub const BASE_LEDGER_BYTES: u64 = 8_388_608;
pub const OPERATION_SLOT_BYTES: u64 = 4_194_304;
pub const MEMORY_PROFILE_32_MIB: u64 = 32 * 1_024 * 1_024;
pub const MEMORY_PROFILE_48_MIB: u64 = 48 * 1_024 * 1_024;
pub const MEMORY_PROFILE_72_MIB: u64 = 72 * 1_024 * 1_024;
const MEMORY_COMPONENT_COUNT: usize = 9;

/// Availability state for one optional direct observation. An unavailable,
/// inapplicable, or deferred observation never carries a fabricated numeric
/// value.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OptionalObservationStatusV1 {
    Observed,
    Unavailable,
    NotApplicable,
    Deferred,
}

/// Authority scope of one observation. This is deliberately independent of
/// the storage-admission policy domains: process/host measurements are
/// evidence only and can never authorize an operation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ObservationScopeV1 {
    Operation,
    Root,
    Process,
    Host,
}

/// Typed optional unsigned observation used at internal semantic ports.
/// Private fields and constructors enforce `value.is_some()` if and only if
/// the status is `Observed`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OptionalU64ObservationV1 {
    status: OptionalObservationStatusV1,
    value: Option<u64>,
    method: &'static str,
    scope: ObservationScopeV1,
}

impl OptionalU64ObservationV1 {
    /// Evidence consumers must be able to distinguish a deliberate method or
    /// unavailability reason from an omitted one.  Empty text would be a
    /// non-reportable substitute just as misleading as an invented numeric
    /// value, so reject it at the crate-private construction boundary.
    const fn require_nonempty_description_v1(description: &'static str) {
        if description.is_empty() {
            panic!("optional observation method/reason must not be empty");
        }
    }

    pub(crate) const fn observed(
        value: u64,
        method: &'static str,
        scope: ObservationScopeV1,
    ) -> Self {
        Self::require_nonempty_description_v1(method);
        Self {
            status: OptionalObservationStatusV1::Observed,
            value: Some(value),
            method,
            scope,
        }
    }

    pub(crate) const fn unavailable(reason: &'static str, scope: ObservationScopeV1) -> Self {
        Self::require_nonempty_description_v1(reason);
        Self {
            status: OptionalObservationStatusV1::Unavailable,
            value: None,
            method: reason,
            scope,
        }
    }

    pub(crate) const fn not_applicable(reason: &'static str, scope: ObservationScopeV1) -> Self {
        Self::require_nonempty_description_v1(reason);
        Self {
            status: OptionalObservationStatusV1::NotApplicable,
            value: None,
            method: reason,
            scope,
        }
    }

    pub(crate) const fn deferred(reason: &'static str, scope: ObservationScopeV1) -> Self {
        Self::require_nonempty_description_v1(reason);
        Self {
            status: OptionalObservationStatusV1::Deferred,
            value: None,
            method: reason,
            scope,
        }
    }

    pub const fn status(self) -> OptionalObservationStatusV1 {
        self.status
    }

    pub const fn value(self) -> Option<u64> {
        self.value
    }

    pub const fn method(self) -> &'static str {
        self.method
    }

    pub const fn scope(self) -> ObservationScopeV1 {
        self.scope
    }

    /// Accumulate homogeneous operation observations without inventing a
    /// number when any constituent is unavailable. The first precise
    /// non-observed status/reason is retained.
    pub(crate) fn checked_add_operation_v1(
        self,
        other: Self,
        observed_method: &'static str,
    ) -> CoreResult<Self> {
        if self.scope != ObservationScopeV1::Operation
            || other.scope != ObservationScopeV1::Operation
        {
            return Err(CoreError::Schema);
        }
        match (self.value, other.value) {
            (Some(left), Some(right)) => Ok(Self::observed(
                left.checked_add(right).ok_or(CoreError::IntegerOverflow)?,
                observed_method,
                ObservationScopeV1::Operation,
            )),
            (None, _) => Ok(self),
            (_, None) => Ok(other),
        }
    }
}

/// Host- and substrate-dependent terminal observations that are deliberately
/// separate from LayerFS logical admission and accounting.  L1.5.5 has no
/// portable direct provider for these values, so every field is explicitly
/// unavailable with an absent numeric value.  Naming the fields here makes
/// that absence reportable on every terminal path instead of hiding it in a
/// prose disclaimer or encoding it as zero.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TerminalOptionalObservationsV1 {
    process_cpu_nanoseconds: OptionalU64ObservationV1,
    allocator_live_bytes: OptionalU64ObservationV1,
    allocator_high_water_bytes: OptionalU64ObservationV1,
    rss_bytes: OptionalU64ObservationV1,
    pss_bytes: OptionalU64ObservationV1,
    page_cache_bytes: OptionalU64ObservationV1,
    process_open_descriptors: OptionalU64ObservationV1,
    host_open_descriptors: OptionalU64ObservationV1,
    filesystem_allocated_bytes: OptionalU64ObservationV1,
    filesystem_allocated_blocks: OptionalU64ObservationV1,
    filesystem_free_bytes: OptionalU64ObservationV1,
    filesystem_quota_bytes: OptionalU64ObservationV1,
    physical_inodes: OptionalU64ObservationV1,
}

impl TerminalOptionalObservationsV1 {
    pub const fn portable_l155_unavailable() -> Self {
        const PROCESS_REASON: &str = "portable direct process observation is unavailable in L1.5.5";
        const HOST_REASON: &str = "portable direct host observation is unavailable in L1.5.5";
        const FILESYSTEM_REASON: &str =
            "portable direct filesystem observation is unavailable in L1.5.5";
        Self {
            process_cpu_nanoseconds: OptionalU64ObservationV1::unavailable(
                PROCESS_REASON,
                ObservationScopeV1::Process,
            ),
            allocator_live_bytes: OptionalU64ObservationV1::unavailable(
                PROCESS_REASON,
                ObservationScopeV1::Process,
            ),
            allocator_high_water_bytes: OptionalU64ObservationV1::unavailable(
                PROCESS_REASON,
                ObservationScopeV1::Process,
            ),
            rss_bytes: OptionalU64ObservationV1::unavailable(
                PROCESS_REASON,
                ObservationScopeV1::Process,
            ),
            pss_bytes: OptionalU64ObservationV1::unavailable(
                PROCESS_REASON,
                ObservationScopeV1::Process,
            ),
            page_cache_bytes: OptionalU64ObservationV1::unavailable(
                PROCESS_REASON,
                ObservationScopeV1::Process,
            ),
            process_open_descriptors: OptionalU64ObservationV1::unavailable(
                PROCESS_REASON,
                ObservationScopeV1::Process,
            ),
            host_open_descriptors: OptionalU64ObservationV1::unavailable(
                HOST_REASON,
                ObservationScopeV1::Host,
            ),
            filesystem_allocated_bytes: OptionalU64ObservationV1::unavailable(
                FILESYSTEM_REASON,
                ObservationScopeV1::Root,
            ),
            filesystem_allocated_blocks: OptionalU64ObservationV1::unavailable(
                FILESYSTEM_REASON,
                ObservationScopeV1::Root,
            ),
            filesystem_free_bytes: OptionalU64ObservationV1::unavailable(
                FILESYSTEM_REASON,
                ObservationScopeV1::Root,
            ),
            filesystem_quota_bytes: OptionalU64ObservationV1::unavailable(
                FILESYSTEM_REASON,
                ObservationScopeV1::Root,
            ),
            physical_inodes: OptionalU64ObservationV1::unavailable(
                FILESYSTEM_REASON,
                ObservationScopeV1::Root,
            ),
        }
    }

    pub const fn process_cpu_nanoseconds(self) -> OptionalU64ObservationV1 {
        self.process_cpu_nanoseconds
    }

    pub const fn allocator_live_bytes(self) -> OptionalU64ObservationV1 {
        self.allocator_live_bytes
    }

    pub const fn allocator_high_water_bytes(self) -> OptionalU64ObservationV1 {
        self.allocator_high_water_bytes
    }

    pub const fn rss_bytes(self) -> OptionalU64ObservationV1 {
        self.rss_bytes
    }

    pub const fn pss_bytes(self) -> OptionalU64ObservationV1 {
        self.pss_bytes
    }

    pub const fn page_cache_bytes(self) -> OptionalU64ObservationV1 {
        self.page_cache_bytes
    }

    pub const fn process_open_descriptors(self) -> OptionalU64ObservationV1 {
        self.process_open_descriptors
    }

    pub const fn host_open_descriptors(self) -> OptionalU64ObservationV1 {
        self.host_open_descriptors
    }

    pub const fn filesystem_allocated_bytes(self) -> OptionalU64ObservationV1 {
        self.filesystem_allocated_bytes
    }

    pub const fn filesystem_allocated_blocks(self) -> OptionalU64ObservationV1 {
        self.filesystem_allocated_blocks
    }

    pub const fn filesystem_free_bytes(self) -> OptionalU64ObservationV1 {
        self.filesystem_free_bytes
    }

    pub const fn filesystem_quota_bytes(self) -> OptionalU64ObservationV1 {
        self.filesystem_quota_bytes
    }

    pub const fn physical_inodes(self) -> OptionalU64ObservationV1 {
        self.physical_inodes
    }

    pub const fn all(self) -> [OptionalU64ObservationV1; 13] {
        [
            self.process_cpu_nanoseconds,
            self.allocator_live_bytes,
            self.allocator_high_water_bytes,
            self.rss_bytes,
            self.pss_bytes,
            self.page_cache_bytes,
            self.process_open_descriptors,
            self.host_open_descriptors,
            self.filesystem_allocated_bytes,
            self.filesystem_allocated_blocks,
            self.filesystem_free_bytes,
            self.filesystem_quota_bytes,
            self.physical_inodes,
        ]
    }
}

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

    #[cfg(test)]
    pub fn reserve_operation(&self) -> CoreResult<OperationReservationV1<'_>> {
        self.reserve_operation_with_plan(OperationMemoryPlanV1::empty())
    }

    #[cfg(test)]
    pub fn reserve_operation_with_plan(
        &self,
        plan: OperationMemoryPlanV1,
    ) -> CoreResult<OperationReservationV1<'_>> {
        let mut reservation = self.reserve_operation_unplanned()?;
        reservation.declare_plan(plan)?;
        Ok(reservation)
    }

    /// Reserve the fixed operation slot before inspecting an operation's
    /// typed request or any supplier/sink declaration. The shared root owner
    /// uses this entry point, then declares the exact named buffer plan while
    /// the already-owned slot remains borrowed through all lower layers.
    pub(crate) fn reserve_operation_unplanned(&self) -> CoreResult<OperationReservationV1<'_>> {
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
                    self.high_water_slots.fetch_max(next, Ordering::AcqRel);
                    return Ok(OperationReservationV1 {
                        ledger: self,
                        released: false,
                        plan_declared: false,
                        planned_bytes: 0,
                        plan: OperationMemoryPlanV1::empty(),
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
    plan_declared: bool,
    planned_bytes: u64,
    plan: OperationMemoryPlanV1,
}

impl OperationReservationV1<'_> {
    pub(crate) fn declare_plan(&mut self, plan: OperationMemoryPlanV1) -> CoreResult<()> {
        if self.plan_declared || plan.total_bytes > OPERATION_SLOT_BYTES {
            return Err(CoreError::ResourceRefused);
        }
        let mut live_before = self.ledger.live_planned_bytes.load(Ordering::Acquire);
        let live = loop {
            let live_after = live_before
                .checked_add(plan.total_bytes)
                .ok_or(CoreError::IntegerOverflow)?;
            match self.ledger.live_planned_bytes.compare_exchange_weak(
                live_before,
                live_after,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => break live_after,
                Err(observed) => live_before = observed,
            }
        };
        self.ledger
            .high_water_planned_bytes
            .fetch_max(live, Ordering::AcqRel);
        self.plan = plan;
        self.planned_bytes = plan.total_bytes;
        self.plan_declared = true;
        Ok(())
    }

    /// Prove that an already-admitted operation owns every named buffer a
    /// lower layer will borrow. This performs no admission and cannot consume
    /// another slot.
    pub(crate) fn require(&self, required: OperationMemoryPlanV1) -> CoreResult<()> {
        if self.plan_declared && self.plan.covers(required) {
            Ok(())
        } else {
            Err(CoreError::ResourceRefused)
        }
    }

    /// Explicitly return the root-owned logical operation slot. The atomics
    /// cannot be poisoned, but checked transitions still make a corrupt or
    /// double release observable to the owning root instead of wrapping an
    /// unsigned counter in Drop.
    pub(crate) fn release_v1(&mut self) -> CoreResult<()> {
        if self.released {
            return Ok(());
        }
        self.ledger
            .live_planned_bytes
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |current| {
                current.checked_sub(self.planned_bytes)
            })
            .map_err(|_| CoreError::ResourceRefused)?;
        if self
            .ledger
            .admitted_slots
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |current| {
                current.checked_sub(1)
            })
            .is_err()
        {
            let restored = self.ledger.live_planned_bytes.fetch_update(
                Ordering::AcqRel,
                Ordering::Acquire,
                |current| current.checked_add(self.planned_bytes),
            );
            debug_assert!(restored.is_ok());
            return Err(CoreError::ResourceRefused);
        }
        self.released = true;
        Ok(())
    }
}

impl Drop for OperationReservationV1<'_> {
    fn drop(&mut self) {
        let _ = self.release_v1();
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
    /// Logical dispositions returned by the operation-local pack sink before
    /// storage admission. Reused candidates are truncated and never become a
    /// physical carrier entry.
    pub pack_local_objects_created: u64,
    pub pack_local_objects_reused: u64,
    /// Canonical object records retained in physical carrier bytes.
    pub physical_carrier_object_writes: u64,
    pub version_objects_created: u64,
    pub version_objects_reused: u64,
    pub tree_objects_created: u64,
    pub tree_objects_reused: u64,
    pub file_objects_created: u64,
    pub file_objects_reused: u64,
    pub symlink_objects_created: u64,
    pub symlink_objects_reused: u64,
    pub chunk_objects_created: u64,
    pub chunk_objects_reused: u64,
    /// Complete-closure objects absent from occupied storage. This is kept
    /// separate from pack-construction dispositions.
    pub closure_objects_missing: u64,
    /// Complete-closure objects found and byte-validated in occupied storage.
    pub closure_objects_occupied_validated: u64,
    pub tree_nodes_created: u64,
    pub tree_nodes_reused: u64,
    pub pack_entries: u64,
    pub pack_bytes: u64,
    pub carrier_bytes_total: u64,
    pub final_carrier_bytes: u64,
    pub maximum_active_carrier_bytes: u64,
    pub carrier_rollovers: u64,
    /// Exact encoded logical bytes installed as immutable carrier names.
    /// This is LayerFS-owned accounting, never filesystem allocated blocks,
    /// writable-layer consumption, quota headroom, or host free space.
    pub installed_carrier_logical_bytes: u64,
    pub temporary_preparation_bytes: u64,
    pub global_seen_lookups: u64,
    pub global_seen_probes: u64,
    pub global_seen_maximum_probe: u64,
    pub global_seen_entries: u64,
    pub global_seen_table_bytes: u64,
    pub global_seen_metadata_bytes_read: u64,
    pub global_seen_metadata_read_calls: u64,
    pub global_seen_metadata_bytes_written: u64,
    pub global_seen_same_carrier_reuses: u64,
    pub global_seen_cross_carrier_reuses: u64,
    /// Direct work performed by bounded file-backed heapsorts and their
    /// canonical-order scans. These are operation events, not estimates from
    /// record counts or spool byte lengths.
    pub file_sort_comparisons: u64,
    pub file_sort_record_reads: u64,
    pub file_sort_record_writes: u64,
    pub file_sort_passes: u64,
    pub file_sort_control_polls: u64,
    pub file_sort_work_units: u64,
    pub file_sort_maximum_work_budget: u64,
    pub file_sort_temporary_bytes_high_water: u64,
    /// Direct phase-one/root-admission events. Queue depth counts waiting
    /// tickets, while active-slot high-water counts granted operations. The
    /// duration is measured with the portable monotonic `Instant` clock and
    /// includes ticket issue through grant or typed refusal.
    pub root_admission_queue_entries: u64,
    pub root_admission_queue_refusals: u64,
    pub root_admission_queue_depth_high_water: u64,
    pub root_admission_active_slots_high_water: u64,
    pub root_admission_wait_polls: u64,
    pub root_admission_wait_nanoseconds: u64,
    pub root_admission_memory_refusals: u64,
    pub root_admission_release_failures: u64,
    /// Exact-counter failure for `root_admission_release_failures`.  The
    /// authority-release cause remains the operation's chronological
    /// terminal; this bounded side channel records that the direct diagnostic
    /// event could not be represented instead of substituting a number or
    /// erasing the observation failure.
    pub(crate) root_admission_release_failure_observation_error: Option<CoreError>,
    /// Direct operation-local observations of the narrow shared-root
    /// visibility fence. Wait and hold durations use the portable monotonic
    /// `Instant` clock; they are never derived from process wall time or a
    /// root-global before/after snapshot.
    pub visibility_lock_acquisitions: u64,
    pub visibility_lock_contended_polls: u64,
    pub visibility_lock_wait_nanoseconds: u64,
    pub visibility_lock_hold_nanoseconds: u64,
    pub visibility_lock_hold_nanoseconds_high_water: u64,
    /// The writer publication transaction is deliberately observed
    /// separately so it cannot be mislabeled as the short visibility fence.
    pub publication_lock_acquisitions: u64,
    pub publication_lock_contended_polls: u64,
    pub publication_lock_wait_nanoseconds: u64,
    pub publication_lock_hold_nanoseconds: u64,
    pub publication_lock_hold_nanoseconds_high_water: u64,
    pub fscas_bytes_read: u64,
    pub fscas_read_calls: u64,
    pub fscas_bytes_written: u64,
    pub fscas_catalog_operations: u64,
    pub locator_installs: u64,
    pub locator_equal_incumbent_reuses: u64,
    pub incumbent_comparison_bytes: u64,
    pub incumbent_comparison_windows: u64,
    pub closure_fences: u64,
    pub unreachable_installed_residue_bytes: u64,
    /// Root-owned logical storage admission. These are namespace accounting
    /// values, not allocated filesystem blocks, quota headroom, RSS, or a
    /// physical-disk observation.
    pub storage_bytes_requested: u64,
    pub storage_bytes_reserved: u64,
    pub storage_bytes_released: u64,
    pub storage_bytes_committed: u64,
    pub storage_bytes_retained: u64,
    /// Shared-root lifetime/overlap high water for active logical byte
    /// reservations. This is not an operation-local peak.
    pub root_storage_active_reserved_bytes_lifetime_high_water: u64,
    pub storage_inodes_requested: u64,
    pub storage_inodes_reserved: u64,
    pub storage_inodes_released: u64,
    pub storage_inodes_committed: u64,
    pub storage_inodes_retained: u64,
    /// Shared-root lifetime/overlap high water for active logical namespace
    /// reservations. This is not an operation-local peak or host free-inode
    /// observation.
    pub root_storage_active_reserved_inodes_lifetime_high_water: u64,
    /// Direct operation-local simultaneous preparation state.
    pub storage_preparation_bytes_high_water: u64,
    pub storage_preparation_inodes_high_water: u64,
    /// Exact operation-local preparation still present after explicit
    /// cleanup. A successful handoff requires both values to be zero.
    pub storage_preparation_bytes_current_after_cleanup: u64,
    pub storage_preparation_inodes_current_after_cleanup: u64,
    pub mutable_preparation_residue_bytes: u64,
    pub mutable_preparation_residue_inodes: u64,
    pub immutable_residue_bytes: u64,
    pub immutable_residue_inodes: u64,
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
    pub retry_attempts: u64,
    pub redispatches: u64,
    pub automatic_fallbacks: u64,
    pub provider_switches: u64,
    pub cdc_switches: u64,
    pub publication_authority_dispatches: u64,
    pub update_to_replace_fallbacks: u64,
    pub full_base_payload_fallbacks: u64,
    pub file_sync_calls: u64,
    pub directory_sync_calls: u64,
    pub wal_or_recovery_operations: u64,
    pub memory_backend_operations: u64,
    pub whole_pack_copies: u64,
    pub filesystem_clone_reflink_operations: u64,
    pub layerfs_created_threads: u64,
    pub rayon_work_units: u64,
    pub source_sized_staging_allocations: u64,
    pub workspace_sized_staging_allocations: u64,
    /// Exact root-owned logical userspace ledger high water. This is not
    /// allocator usage, RSS, PSS, stack, or page-cache residency.
    pub memory_high_water: u64,
    /// Direct high water of LayerFS-owned `File` values at instrumented
    /// ownership sites. This is not a process descriptor-table observation.
    pub layerfs_open_file_handles_high_water: u64,
}

impl OperationCountersV1 {
    /// Typed availability for host-dependent terminal evidence. This is
    /// callable after success or failure and never changes logical counters.
    pub const fn terminal_optional_observations_v1(&self) -> TerminalOptionalObservationsV1 {
        TerminalOptionalObservationsV1::portable_l155_unavailable()
    }

    /// Checked accumulation for independently borrowed lower-layer work.
    /// Scalar work adds; high-water observations take their maximum.
    pub(crate) fn accumulate(&mut self, other: Self) -> CoreResult<()> {
        // A terminal observation is one indivisible record. A checked
        // failure in a late scalar/current-state field must not expose the
        // earlier half of this merge while leaving the remaining fields at
        // their old values.
        let mut checked = *self;
        match checked.accumulate_in_place_v1(other) {
            Ok(()) => {
                *self = checked;
                Ok(())
            }
            Err(error) => {
                // The bounded release-observation side channel is evidence
                // about why the otherwise-transactional record could not be
                // represented. Preserve it without exposing any partially
                // added scalar or high-water field.
                self.root_admission_release_failure_observation_error = self
                    .root_admission_release_failure_observation_error
                    .or(other.root_admission_release_failure_observation_error)
                    .or(checked.root_admission_release_failure_observation_error);
                Err(error)
            }
        }
    }

    fn accumulate_in_place_v1(&mut self, other: Self) -> CoreResult<()> {
        macro_rules! add_fields {
            ($($field:ident),+ $(,)?) => {
                $(self.$field = self.$field.checked_add(other.$field)
                    .ok_or(CoreError::IntegerOverflow)?;)+
            };
        }
        add_fields!(
            bytes_read,
            source_read_calls,
            source_bytes_read,
            bytes_written,
            bytes_copied,
            bytes_boundary_inspected,
            ring_fills,
            ring_wrap_spans,
            cdc_scan_calls,
            cdc_scan_bytes,
            seqcdc_comparisons,
            seqcdc_equal_absorptions,
            seqcdc_opposing_slopes,
            seqcdc_jumps,
            seqcdc_jump_bytes,
            logical_hash_bytes,
            logical_hash_update_calls,
            physical_hash_bytes,
            physical_hash_update_calls,
            bytes_structurally_reused,
            logical_chunks_created,
            logical_chunks_reused,
            physical_objects_created,
            physical_objects_reused,
            pack_local_objects_created,
            pack_local_objects_reused,
            physical_carrier_object_writes,
            version_objects_created,
            version_objects_reused,
            tree_objects_created,
            tree_objects_reused,
            file_objects_created,
            file_objects_reused,
            symlink_objects_created,
            symlink_objects_reused,
            chunk_objects_created,
            chunk_objects_reused,
            closure_objects_missing,
            closure_objects_occupied_validated,
            tree_nodes_created,
            tree_nodes_reused,
            pack_entries,
            pack_bytes,
            carrier_bytes_total,
            carrier_rollovers,
            installed_carrier_logical_bytes,
            temporary_preparation_bytes,
            global_seen_lookups,
            global_seen_probes,
            global_seen_metadata_bytes_read,
            global_seen_metadata_read_calls,
            global_seen_metadata_bytes_written,
            global_seen_same_carrier_reuses,
            global_seen_cross_carrier_reuses,
            file_sort_comparisons,
            file_sort_record_reads,
            file_sort_record_writes,
            file_sort_passes,
            file_sort_control_polls,
            file_sort_work_units,
            root_admission_queue_entries,
            root_admission_queue_refusals,
            root_admission_wait_polls,
            root_admission_wait_nanoseconds,
            root_admission_memory_refusals,
            visibility_lock_acquisitions,
            visibility_lock_contended_polls,
            visibility_lock_wait_nanoseconds,
            visibility_lock_hold_nanoseconds,
            publication_lock_acquisitions,
            publication_lock_contended_polls,
            publication_lock_wait_nanoseconds,
            publication_lock_hold_nanoseconds,
            fscas_bytes_read,
            fscas_read_calls,
            fscas_bytes_written,
            fscas_catalog_operations,
            locator_installs,
            locator_equal_incumbent_reuses,
            incumbent_comparison_bytes,
            incumbent_comparison_windows,
            closure_fences,
            unreachable_installed_residue_bytes,
            storage_bytes_requested,
            storage_bytes_reserved,
            storage_bytes_released,
            storage_bytes_committed,
            storage_bytes_retained,
            storage_inodes_requested,
            storage_inodes_reserved,
            storage_inodes_released,
            storage_inodes_committed,
            storage_inodes_retained,
            mutable_preparation_residue_bytes,
            mutable_preparation_residue_inodes,
            immutable_residue_bytes,
            immutable_residue_inodes,
            update_base_payload_bytes,
            update_inserted_bytes,
            update_reference_metadata_records,
            update_reference_metadata_bytes,
            update_resynchronization_bytes,
            anchor_attempts,
            exact_rejoin_bytes,
            rejoin_successes,
            rejoin_failures,
            update_failures,
            retry_attempts,
            redispatches,
            automatic_fallbacks,
            provider_switches,
            cdc_switches,
            publication_authority_dispatches,
            update_to_replace_fallbacks,
            full_base_payload_fallbacks,
            file_sync_calls,
            directory_sync_calls,
            wal_or_recovery_operations,
            memory_backend_operations,
            whole_pack_copies,
            filesystem_clone_reflink_operations,
            layerfs_created_threads,
            rayon_work_units,
            source_sized_staging_allocations,
            workspace_sized_staging_allocations,
        );
        let release_failure_observation_error = self
            .root_admission_release_failure_observation_error
            .or(other.root_admission_release_failure_observation_error);
        self.root_admission_release_failures = match self
            .root_admission_release_failures
            .checked_add(other.root_admission_release_failures)
        {
            Some(total) => total,
            None => {
                self.root_admission_release_failure_observation_error =
                    Some(CoreError::IntegerOverflow);
                return Err(CoreError::IntegerOverflow);
            }
        };
        self.root_admission_release_failure_observation_error = release_failure_observation_error;
        self.memory_high_water = self.memory_high_water.max(other.memory_high_water);
        self.final_carrier_bytes = self.final_carrier_bytes.max(other.final_carrier_bytes);
        self.maximum_active_carrier_bytes = self
            .maximum_active_carrier_bytes
            .max(other.maximum_active_carrier_bytes);
        self.global_seen_maximum_probe = self
            .global_seen_maximum_probe
            .max(other.global_seen_maximum_probe);
        self.global_seen_entries = self.global_seen_entries.max(other.global_seen_entries);
        self.global_seen_table_bytes = self
            .global_seen_table_bytes
            .max(other.global_seen_table_bytes);
        self.file_sort_maximum_work_budget = self
            .file_sort_maximum_work_budget
            .max(other.file_sort_maximum_work_budget);
        self.file_sort_temporary_bytes_high_water = self
            .file_sort_temporary_bytes_high_water
            .max(other.file_sort_temporary_bytes_high_water);
        self.root_admission_queue_depth_high_water = self
            .root_admission_queue_depth_high_water
            .max(other.root_admission_queue_depth_high_water);
        self.root_admission_active_slots_high_water = self
            .root_admission_active_slots_high_water
            .max(other.root_admission_active_slots_high_water);
        self.visibility_lock_hold_nanoseconds_high_water = self
            .visibility_lock_hold_nanoseconds_high_water
            .max(other.visibility_lock_hold_nanoseconds_high_water);
        self.publication_lock_hold_nanoseconds_high_water = self
            .publication_lock_hold_nanoseconds_high_water
            .max(other.publication_lock_hold_nanoseconds_high_water);
        self.root_storage_active_reserved_bytes_lifetime_high_water = self
            .root_storage_active_reserved_bytes_lifetime_high_water
            .max(other.root_storage_active_reserved_bytes_lifetime_high_water);
        self.root_storage_active_reserved_inodes_lifetime_high_water = self
            .root_storage_active_reserved_inodes_lifetime_high_water
            .max(other.root_storage_active_reserved_inodes_lifetime_high_water);
        self.storage_preparation_bytes_high_water = self
            .storage_preparation_bytes_high_water
            .max(other.storage_preparation_bytes_high_water);
        self.storage_preparation_inodes_high_water = self
            .storage_preparation_inodes_high_water
            .max(other.storage_preparation_inodes_high_water);
        self.storage_preparation_bytes_current_after_cleanup = self
            .storage_preparation_bytes_current_after_cleanup
            .checked_add(other.storage_preparation_bytes_current_after_cleanup)
            .ok_or(CoreError::IntegerOverflow)?;
        self.storage_preparation_inodes_current_after_cleanup = self
            .storage_preparation_inodes_current_after_cleanup
            .checked_add(other.storage_preparation_inodes_current_after_cleanup)
            .ok_or(CoreError::IntegerOverflow)?;
        self.layerfs_open_file_handles_high_water = self
            .layerfs_open_file_handles_high_water
            .max(other.layerfs_open_file_handles_high_water);
        Ok(())
    }

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
        let mut checked = *self;
        checked.add(CounterFieldV1::RingFills, counters.ring_fills)?;
        checked.add(CounterFieldV1::RingWrapSpans, counters.ring_wrap_spans)?;
        checked.add(CounterFieldV1::CdcScanCalls, counters.scan_calls)?;
        checked.add(CounterFieldV1::CdcScanBytes, counters.scan_bytes)?;
        checked.add(
            CounterFieldV1::BytesBoundaryInspected,
            counters.boundary_inspected_bytes,
        )?;
        *self = checked;
        Ok(())
    }

    pub fn add_seqcdc(&mut self, counters: crate::cdc::SeqCdcCountersV1) -> CoreResult<()> {
        let mut checked = *self;
        checked.add(CounterFieldV1::SeqCdcComparisons, counters.comparisons)?;
        checked.add(
            CounterFieldV1::SeqCdcEqualAbsorptions,
            counters.equal_absorptions,
        )?;
        checked.add(
            CounterFieldV1::SeqCdcOpposingSlopes,
            counters.opposing_slopes,
        )?;
        checked.add(CounterFieldV1::SeqCdcJumps, counters.jumps)?;
        checked.add(CounterFieldV1::SeqCdcJumpBytes, counters.jump_bytes)?;
        *self = checked;
        Ok(())
    }

    pub fn record_pack_storage(
        &mut self,
        installed_carrier_logical_bytes: u64,
        temporary_bytes: u64,
    ) -> CoreResult<()> {
        let mut checked = *self;
        checked.add(
            CounterFieldV1::InstalledCarrierLogicalBytes,
            installed_carrier_logical_bytes,
        )?;
        checked.add(CounterFieldV1::TemporaryPreparationBytes, temporary_bytes)?;
        *self = checked;
        Ok(())
    }

    pub(crate) fn record_carrier(&mut self, bytes: u64, rollover: bool) -> CoreResult<()> {
        let mut checked = *self;
        checked.add(CounterFieldV1::CarrierBytesTotal, bytes)?;
        if rollover {
            checked.add(CounterFieldV1::CarrierRollovers, 1)?;
        }
        checked.final_carrier_bytes = bytes;
        checked.maximum_active_carrier_bytes = checked.maximum_active_carrier_bytes.max(bytes);
        *self = checked;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn record_global_seen(
        &mut self,
        lookups: u64,
        probes: u64,
        maximum_probe: u32,
        entries: u32,
        table_bytes: u64,
        metadata_bytes_read: u64,
        metadata_read_calls: u64,
        metadata_bytes_written: u64,
    ) -> CoreResult<()> {
        let mut checked = *self;
        checked.add(CounterFieldV1::GlobalSeenLookups, lookups)?;
        checked.add(CounterFieldV1::GlobalSeenProbes, probes)?;
        checked.add(
            CounterFieldV1::GlobalSeenMetadataBytesRead,
            metadata_bytes_read,
        )?;
        checked.add(
            CounterFieldV1::GlobalSeenMetadataReadCalls,
            metadata_read_calls,
        )?;
        checked.add(
            CounterFieldV1::GlobalSeenMetadataBytesWritten,
            metadata_bytes_written,
        )?;
        checked.global_seen_maximum_probe = checked
            .global_seen_maximum_probe
            .max(u64::from(maximum_probe));
        checked.global_seen_entries = checked.global_seen_entries.max(u64::from(entries));
        checked.global_seen_table_bytes = checked.global_seen_table_bytes.max(table_bytes);
        *self = checked;
        Ok(())
    }

    fn record_file_sort_event_v1(&mut self, event: FileSortEventV1) -> CoreResult<()> {
        let mut checked = *self;
        checked.file_sort_work_units = checked
            .file_sort_work_units
            .checked_add(1)
            .ok_or(CoreError::IntegerOverflow)?;
        let target = match event {
            FileSortEventV1::Comparison => &mut checked.file_sort_comparisons,
            FileSortEventV1::RecordRead => &mut checked.file_sort_record_reads,
            FileSortEventV1::RecordWrite => &mut checked.file_sort_record_writes,
        };
        *target = target.checked_add(1).ok_or(CoreError::IntegerOverflow)?;
        *self = checked;
        Ok(())
    }

    fn record_file_sort_pass_v1(&mut self) -> CoreResult<()> {
        self.file_sort_passes = self
            .file_sort_passes
            .checked_add(1)
            .ok_or(CoreError::IntegerOverflow)?;
        Ok(())
    }

    fn record_file_sort_control_poll_v1(&mut self) -> CoreResult<()> {
        self.file_sort_control_polls = self
            .file_sort_control_polls
            .checked_add(1)
            .ok_or(CoreError::IntegerOverflow)?;
        Ok(())
    }

    pub(crate) fn record_root_admission_queue_entry_v1(
        &mut self,
        waiting_depth: u64,
    ) -> CoreResult<()> {
        self.root_admission_queue_entries = self
            .root_admission_queue_entries
            .checked_add(1)
            .ok_or(CoreError::IntegerOverflow)?;
        self.root_admission_queue_depth_high_water = self
            .root_admission_queue_depth_high_water
            .max(waiting_depth);
        Ok(())
    }

    pub(crate) fn record_root_admission_queue_refusal_v1(&mut self) -> CoreResult<()> {
        self.root_admission_queue_refusals = self
            .root_admission_queue_refusals
            .checked_add(1)
            .ok_or(CoreError::IntegerOverflow)?;
        Ok(())
    }

    pub(crate) fn record_root_admission_grant_v1(&mut self, active_slots: u64) -> CoreResult<()> {
        self.root_admission_active_slots_high_water = self
            .root_admission_active_slots_high_water
            .max(active_slots);
        Ok(())
    }

    pub(crate) fn record_root_admission_wait_poll_v1(&mut self) -> CoreResult<()> {
        self.root_admission_wait_polls = self
            .root_admission_wait_polls
            .checked_add(1)
            .ok_or(CoreError::IntegerOverflow)?;
        Ok(())
    }

    pub(crate) fn record_root_admission_wait_v1(&mut self, nanoseconds: u64) -> CoreResult<()> {
        self.root_admission_wait_nanoseconds = self
            .root_admission_wait_nanoseconds
            .checked_add(nanoseconds)
            .ok_or(CoreError::IntegerOverflow)?;
        Ok(())
    }

    pub(crate) fn record_root_admission_memory_refusal_v1(&mut self) -> CoreResult<()> {
        self.root_admission_memory_refusals = self
            .root_admission_memory_refusals
            .checked_add(1)
            .ok_or(CoreError::IntegerOverflow)?;
        Ok(())
    }

    pub(crate) fn record_root_admission_release_failure_v1(&mut self) -> CoreResult<()> {
        self.root_admission_release_failures =
            match self.root_admission_release_failures.checked_add(1) {
                Some(total) => total,
                None => {
                    self.root_admission_release_failure_observation_error =
                        Some(CoreError::IntegerOverflow);
                    return Err(CoreError::IntegerOverflow);
                }
            };
        Ok(())
    }

    /// Typed evidence that the exact admission-release-failure counter could
    /// not represent a required event.  `None` means the scalar remains exact;
    /// `Some` never authorizes a numeric replacement and requires the owning
    /// terminal path to fail closed.
    pub(crate) const fn root_admission_release_failure_observation_error_v1(
        &self,
    ) -> Option<CoreError> {
        self.root_admission_release_failure_observation_error
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn record_root_lock_observations_v1(
        &mut self,
        visibility_acquisitions: u64,
        visibility_contended_polls: u64,
        visibility_wait_nanoseconds: u64,
        visibility_hold_nanoseconds: u64,
        visibility_maximum_hold_nanoseconds: u64,
        publication_acquisitions: u64,
        publication_contended_polls: u64,
        publication_wait_nanoseconds: u64,
        publication_hold_nanoseconds: u64,
        publication_maximum_hold_nanoseconds: u64,
    ) -> CoreResult<()> {
        let mut checked = *self;
        macro_rules! checked_add {
            ($field:ident, $value:expr) => {
                checked.$field = checked
                    .$field
                    .checked_add($value)
                    .ok_or(CoreError::IntegerOverflow)?;
            };
        }
        checked_add!(visibility_lock_acquisitions, visibility_acquisitions);
        checked_add!(visibility_lock_contended_polls, visibility_contended_polls);
        checked_add!(
            visibility_lock_wait_nanoseconds,
            visibility_wait_nanoseconds
        );
        checked_add!(
            visibility_lock_hold_nanoseconds,
            visibility_hold_nanoseconds
        );
        checked.visibility_lock_hold_nanoseconds_high_water = checked
            .visibility_lock_hold_nanoseconds_high_water
            .max(visibility_maximum_hold_nanoseconds);
        checked_add!(publication_lock_acquisitions, publication_acquisitions);
        checked_add!(
            publication_lock_contended_polls,
            publication_contended_polls
        );
        checked_add!(
            publication_lock_wait_nanoseconds,
            publication_wait_nanoseconds
        );
        checked_add!(
            publication_lock_hold_nanoseconds,
            publication_hold_nanoseconds
        );
        checked.publication_lock_hold_nanoseconds_high_water = checked
            .publication_lock_hold_nanoseconds_high_water
            .max(publication_maximum_hold_nanoseconds);
        *self = checked;
        Ok(())
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

    pub(crate) fn record_locator_install(&mut self) -> CoreResult<()> {
        self.add(CounterFieldV1::LocatorInstalls, 1)
    }

    pub(crate) fn record_locator_equal_incumbent_reuse(&mut self) -> CoreResult<()> {
        self.add(CounterFieldV1::LocatorEqualIncumbentReuses, 1)
    }

    pub(crate) fn record_pack_object_disposition(
        &mut self,
        kind: PhysicalObjectKindV1,
        created: bool,
    ) -> CoreResult<()> {
        let mut checked = *self;
        checked.add(
            if created {
                CounterFieldV1::PackLocalObjectsCreated
            } else {
                CounterFieldV1::PackLocalObjectsReused
            },
            1,
        )?;
        if created {
            checked.add(CounterFieldV1::PhysicalCarrierObjectWrites, 1)?;
        }
        let target = match (kind, created) {
            (PhysicalObjectKindV1::VersionRecord, true) => &mut checked.version_objects_created,
            (PhysicalObjectKindV1::VersionRecord, false) => &mut checked.version_objects_reused,
            (PhysicalObjectKindV1::Tree, true) => &mut checked.tree_objects_created,
            (PhysicalObjectKindV1::Tree, false) => &mut checked.tree_objects_reused,
            (PhysicalObjectKindV1::File, true) => &mut checked.file_objects_created,
            (PhysicalObjectKindV1::File, false) => &mut checked.file_objects_reused,
            (PhysicalObjectKindV1::Symlink, true) => &mut checked.symlink_objects_created,
            (PhysicalObjectKindV1::Symlink, false) => &mut checked.symlink_objects_reused,
            (PhysicalObjectKindV1::Chunk, true) => &mut checked.chunk_objects_created,
            (PhysicalObjectKindV1::Chunk, false) => &mut checked.chunk_objects_reused,
        };
        *target = target.checked_add(1).ok_or(CoreError::IntegerOverflow)?;
        *self = checked;
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn saturate_pack_object_disposition_for_test_v1(
        &mut self,
        kind: PhysicalObjectKindV1,
        created: bool,
    ) {
        let target = match (kind, created) {
            (PhysicalObjectKindV1::VersionRecord, true) => &mut self.version_objects_created,
            (PhysicalObjectKindV1::VersionRecord, false) => &mut self.version_objects_reused,
            (PhysicalObjectKindV1::Tree, true) => &mut self.tree_objects_created,
            (PhysicalObjectKindV1::Tree, false) => &mut self.tree_objects_reused,
            (PhysicalObjectKindV1::File, true) => &mut self.file_objects_created,
            (PhysicalObjectKindV1::File, false) => &mut self.file_objects_reused,
            (PhysicalObjectKindV1::Symlink, true) => &mut self.symlink_objects_created,
            (PhysicalObjectKindV1::Symlink, false) => &mut self.symlink_objects_reused,
            (PhysicalObjectKindV1::Chunk, true) => &mut self.chunk_objects_created,
            (PhysicalObjectKindV1::Chunk, false) => &mut self.chunk_objects_reused,
        };
        *target = u64::MAX;
    }

    pub fn record_incumbent_comparison(&mut self, bytes: u64, windows: u64) -> CoreResult<()> {
        let mut checked = *self;
        checked.add(CounterFieldV1::IncumbentComparisonBytes, bytes)?;
        checked.add(CounterFieldV1::IncumbentComparisonWindows, windows)?;
        *self = checked;
        Ok(())
    }

    pub fn record_closure_fence(&mut self) -> CoreResult<()> {
        self.add(CounterFieldV1::ClosureFences, 1)
    }

    pub fn record_unreachable_installed_residue(&mut self, bytes: u64) -> CoreResult<()> {
        self.add(CounterFieldV1::UnreachableInstalledResidueBytes, bytes)
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn record_storage_admission_v1(
        &mut self,
        requested_bytes: u64,
        reserved_bytes: u64,
        released_bytes: u64,
        committed_bytes: u64,
        retained_bytes: u64,
        requested_inodes: u64,
        reserved_inodes: u64,
        released_inodes: u64,
        committed_inodes: u64,
        retained_inodes: u64,
        root_active_reserved_bytes_lifetime_high_water: u64,
        root_active_reserved_inodes_lifetime_high_water: u64,
        preparation_bytes_high_water: u64,
        preparation_inodes_high_water: u64,
        preparation_bytes_current_after_cleanup: u64,
        preparation_inodes_current_after_cleanup: u64,
        mutable_preparation_residue_bytes: u64,
        mutable_preparation_residue_inodes: u64,
        immutable_residue_bytes: u64,
        immutable_residue_inodes: u64,
    ) -> CoreResult<()> {
        let mut checked = *self;
        macro_rules! checked_add {
            ($field:ident, $value:expr) => {
                checked.$field = checked
                    .$field
                    .checked_add($value)
                    .ok_or(CoreError::IntegerOverflow)?;
            };
        }
        checked_add!(storage_bytes_requested, requested_bytes);
        checked_add!(storage_bytes_reserved, reserved_bytes);
        checked_add!(storage_bytes_released, released_bytes);
        checked_add!(storage_bytes_committed, committed_bytes);
        checked_add!(storage_bytes_retained, retained_bytes);
        checked_add!(storage_inodes_requested, requested_inodes);
        checked_add!(storage_inodes_reserved, reserved_inodes);
        checked_add!(storage_inodes_released, released_inodes);
        checked_add!(storage_inodes_committed, committed_inodes);
        checked_add!(storage_inodes_retained, retained_inodes);
        checked_add!(
            mutable_preparation_residue_bytes,
            mutable_preparation_residue_bytes
        );
        checked_add!(
            mutable_preparation_residue_inodes,
            mutable_preparation_residue_inodes
        );
        checked_add!(immutable_residue_bytes, immutable_residue_bytes);
        checked_add!(immutable_residue_inodes, immutable_residue_inodes);
        checked_add!(
            storage_preparation_bytes_current_after_cleanup,
            preparation_bytes_current_after_cleanup
        );
        checked_add!(
            storage_preparation_inodes_current_after_cleanup,
            preparation_inodes_current_after_cleanup
        );
        checked.root_storage_active_reserved_bytes_lifetime_high_water = checked
            .root_storage_active_reserved_bytes_lifetime_high_water
            .max(root_active_reserved_bytes_lifetime_high_water);
        checked.root_storage_active_reserved_inodes_lifetime_high_water = checked
            .root_storage_active_reserved_inodes_lifetime_high_water
            .max(root_active_reserved_inodes_lifetime_high_water);
        checked.storage_preparation_bytes_high_water = checked
            .storage_preparation_bytes_high_water
            .max(preparation_bytes_high_water);
        checked.storage_preparation_inodes_high_water = checked
            .storage_preparation_inodes_high_water
            .max(preparation_inodes_high_water);
        *self = checked;
        Ok(())
    }

    /// A complete operation may finish its storage ledger successfully and
    /// then discover that the outer root-admission capability cannot be
    /// released. No handoff can cross that failed terminal boundary, so the
    /// operation-relative immutable set is retained residue rather than a
    /// committed success. Reclassify the already-checked terminal observation
    /// atomically without changing the root's immutable byte ownership.
    pub(crate) fn reclassify_storage_commit_as_retained_v1(&mut self) -> CoreResult<()> {
        let mut checked = *self;
        let bytes = checked.storage_bytes_committed;
        let inodes = checked.storage_inodes_committed;
        checked.storage_bytes_retained = checked
            .storage_bytes_retained
            .checked_add(bytes)
            .ok_or(CoreError::IntegerOverflow)?;
        checked.storage_inodes_retained = checked
            .storage_inodes_retained
            .checked_add(inodes)
            .ok_or(CoreError::IntegerOverflow)?;
        checked.immutable_residue_bytes = checked
            .immutable_residue_bytes
            .checked_add(bytes)
            .ok_or(CoreError::IntegerOverflow)?;
        checked.immutable_residue_inodes = checked
            .immutable_residue_inodes
            .checked_add(inodes)
            .ok_or(CoreError::IntegerOverflow)?;
        checked.unreachable_installed_residue_bytes = checked
            .unreachable_installed_residue_bytes
            .checked_add(bytes)
            .ok_or(CoreError::IntegerOverflow)?;
        checked.storage_bytes_committed = 0;
        checked.storage_inodes_committed = 0;
        *self = checked;
        Ok(())
    }

    pub fn observe_layerfs_open_file_handles(&mut self, open_files: u64) {
        self.layerfs_open_file_handles_high_water =
            self.layerfs_open_file_handles_high_water.max(open_files);
    }

    pub fn record_update_base_payload(&mut self, bytes: u64) -> CoreResult<()> {
        self.add(CounterFieldV1::UpdateBasePayloadBytes, bytes)
    }

    pub fn record_update_inserted(&mut self, bytes: u64) -> CoreResult<()> {
        self.add(CounterFieldV1::UpdateInsertedBytes, bytes)
    }

    pub fn record_update_reference_metadata(&mut self, records: u64, bytes: u64) -> CoreResult<()> {
        let mut checked = *self;
        checked.add(CounterFieldV1::UpdateReferenceMetadataRecords, records)?;
        checked.add(CounterFieldV1::UpdateReferenceMetadataBytes, bytes)?;
        *self = checked;
        Ok(())
    }

    pub fn record_exact_rejoin(&mut self, bytes: u64, equal: bool) -> CoreResult<()> {
        let mut checked = *self;
        checked.add(CounterFieldV1::ExactRejoinBytes, bytes)?;
        checked.add(
            if equal {
                CounterFieldV1::RejoinSuccesses
            } else {
                CounterFieldV1::RejoinFailures
            },
            1,
        )?;
        *self = checked;
        Ok(())
    }

    pub fn record_retry_attempt(&mut self) -> CoreResult<()> {
        self.add(CounterFieldV1::RetryAttempts, 1)
    }

    pub fn record_redispatch(&mut self) -> CoreResult<()> {
        self.add(CounterFieldV1::Redispatches, 1)
    }

    pub fn record_fallback_attempt(&mut self) -> CoreResult<()> {
        self.add(CounterFieldV1::AutomaticFallbacks, 1)
    }

    pub fn record_publication_dispatch(&mut self) -> CoreResult<()> {
        self.add(CounterFieldV1::PublicationAuthorityDispatches, 1)
    }

    pub fn record_provider_switch(&mut self) -> CoreResult<()> {
        self.add(CounterFieldV1::ProviderSwitches, 1)
    }

    pub fn record_cdc_switch(&mut self) -> CoreResult<()> {
        self.add(CounterFieldV1::CdcSwitches, 1)
    }

    pub fn record_update_to_replace_fallback(&mut self) -> CoreResult<()> {
        self.add(CounterFieldV1::UpdateToReplaceFallbacks, 1)
    }

    pub fn record_full_base_payload_fallback(&mut self) -> CoreResult<()> {
        self.add(CounterFieldV1::FullBasePayloadFallbacks, 1)
    }

    pub fn record_file_sync(&mut self) -> CoreResult<()> {
        self.add(CounterFieldV1::FileSyncCalls, 1)
    }

    pub fn record_directory_sync(&mut self) -> CoreResult<()> {
        self.add(CounterFieldV1::DirectorySyncCalls, 1)
    }

    pub fn record_wal_or_recovery_operation(&mut self) -> CoreResult<()> {
        self.add(CounterFieldV1::WalOrRecoveryOperations, 1)
    }

    pub fn record_memory_backend_operation(&mut self) -> CoreResult<()> {
        self.add(CounterFieldV1::MemoryBackendOperations, 1)
    }

    pub fn record_whole_pack_copy(&mut self) -> CoreResult<()> {
        self.add(CounterFieldV1::WholePackCopies, 1)
    }

    pub fn record_filesystem_clone_reflink_operation(&mut self) -> CoreResult<()> {
        self.add(CounterFieldV1::FilesystemCloneReflinkOperations, 1)
    }

    pub fn record_layerfs_created_thread(&mut self) -> CoreResult<()> {
        self.add(CounterFieldV1::LayerFsCreatedThreads, 1)
    }

    pub fn record_rayon_work(&mut self) -> CoreResult<()> {
        self.add(CounterFieldV1::RayonWorkUnits, 1)
    }

    pub fn record_source_sized_staging_allocation(&mut self) -> CoreResult<()> {
        self.add(CounterFieldV1::SourceSizedStagingAllocations, 1)
    }

    pub fn record_workspace_sized_staging_allocation(&mut self) -> CoreResult<()> {
        self.add(CounterFieldV1::WorkspaceSizedStagingAllocations, 1)
    }

    pub const fn has_zero_forbidden_work(&self) -> bool {
        self.retry_attempts == 0
            && self.redispatches == 0
            && self.automatic_fallbacks == 0
            && self.provider_switches == 0
            && self.cdc_switches == 0
            && self.publication_authority_dispatches == 0
            && self.update_to_replace_fallbacks == 0
            && self.full_base_payload_fallbacks == 0
            && self.file_sync_calls == 0
            && self.directory_sync_calls == 0
            && self.wal_or_recovery_operations == 0
            && self.memory_backend_operations == 0
            && self.whole_pack_copies == 0
            && self.filesystem_clone_reflink_operations == 0
            && self.layerfs_created_threads == 0
            && self.rayon_work_units == 0
            && self.source_sized_staging_allocations == 0
            && self.workspace_sized_staging_allocations == 0
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
            CounterFieldV1::PackLocalObjectsCreated => &mut self.pack_local_objects_created,
            CounterFieldV1::PackLocalObjectsReused => &mut self.pack_local_objects_reused,
            CounterFieldV1::PhysicalCarrierObjectWrites => &mut self.physical_carrier_object_writes,
            CounterFieldV1::ClosureObjectsMissing => &mut self.closure_objects_missing,
            CounterFieldV1::ClosureObjectsOccupiedValidated => {
                &mut self.closure_objects_occupied_validated
            }
            CounterFieldV1::TreeNodesCreated => &mut self.tree_nodes_created,
            CounterFieldV1::TreeNodesReused => &mut self.tree_nodes_reused,
            CounterFieldV1::PackEntries => &mut self.pack_entries,
            CounterFieldV1::PackBytes => &mut self.pack_bytes,
            CounterFieldV1::CarrierBytesTotal => &mut self.carrier_bytes_total,
            CounterFieldV1::CarrierRollovers => &mut self.carrier_rollovers,
            CounterFieldV1::InstalledCarrierLogicalBytes => {
                &mut self.installed_carrier_logical_bytes
            }
            CounterFieldV1::TemporaryPreparationBytes => &mut self.temporary_preparation_bytes,
            CounterFieldV1::GlobalSeenLookups => &mut self.global_seen_lookups,
            CounterFieldV1::GlobalSeenProbes => &mut self.global_seen_probes,
            CounterFieldV1::GlobalSeenMetadataBytesRead => {
                &mut self.global_seen_metadata_bytes_read
            }
            CounterFieldV1::GlobalSeenMetadataReadCalls => {
                &mut self.global_seen_metadata_read_calls
            }
            CounterFieldV1::GlobalSeenMetadataBytesWritten => {
                &mut self.global_seen_metadata_bytes_written
            }
            CounterFieldV1::GlobalSeenSameCarrierReuses => {
                &mut self.global_seen_same_carrier_reuses
            }
            CounterFieldV1::GlobalSeenCrossCarrierReuses => {
                &mut self.global_seen_cross_carrier_reuses
            }
            CounterFieldV1::FsCasBytesRead => &mut self.fscas_bytes_read,
            CounterFieldV1::FsCasReadCalls => &mut self.fscas_read_calls,
            CounterFieldV1::FsCasBytesWritten => &mut self.fscas_bytes_written,
            CounterFieldV1::FsCasCatalogOperations => &mut self.fscas_catalog_operations,
            CounterFieldV1::LocatorInstalls => &mut self.locator_installs,
            CounterFieldV1::LocatorEqualIncumbentReuses => &mut self.locator_equal_incumbent_reuses,
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
            CounterFieldV1::RetryAttempts => &mut self.retry_attempts,
            CounterFieldV1::Redispatches => &mut self.redispatches,
            CounterFieldV1::AutomaticFallbacks => &mut self.automatic_fallbacks,
            CounterFieldV1::ProviderSwitches => &mut self.provider_switches,
            CounterFieldV1::CdcSwitches => &mut self.cdc_switches,
            CounterFieldV1::PublicationAuthorityDispatches => {
                &mut self.publication_authority_dispatches
            }
            CounterFieldV1::UpdateToReplaceFallbacks => &mut self.update_to_replace_fallbacks,
            CounterFieldV1::FullBasePayloadFallbacks => &mut self.full_base_payload_fallbacks,
            CounterFieldV1::FileSyncCalls => &mut self.file_sync_calls,
            CounterFieldV1::DirectorySyncCalls => &mut self.directory_sync_calls,
            CounterFieldV1::WalOrRecoveryOperations => &mut self.wal_or_recovery_operations,
            CounterFieldV1::MemoryBackendOperations => &mut self.memory_backend_operations,
            CounterFieldV1::WholePackCopies => &mut self.whole_pack_copies,
            CounterFieldV1::FilesystemCloneReflinkOperations => {
                &mut self.filesystem_clone_reflink_operations
            }
            CounterFieldV1::LayerFsCreatedThreads => &mut self.layerfs_created_threads,
            CounterFieldV1::RayonWorkUnits => &mut self.rayon_work_units,
            CounterFieldV1::SourceSizedStagingAllocations => {
                &mut self.source_sized_staging_allocations
            }
            CounterFieldV1::WorkspaceSizedStagingAllocations => {
                &mut self.workspace_sized_staging_allocations
            }
        };
        *target = target
            .checked_add(amount)
            .ok_or(CoreError::IntegerOverflow)?;
        Ok(())
    }
}

/// Neutral cooperative control used by bounded work owned below lifecycle.
/// It deliberately carries no filesystem, CDC, scheduling, or fallback API.
pub(crate) trait OperationWorkControlV1 {
    fn cancellation_requested_v1(&mut self) -> bool;
    fn deadline_exceeded_v1(&mut self) -> bool;
}

/// At most this many directly counted sort work events may occur between
/// cancellation/deadline observations. The initial and terminal polls make
/// the bound apply to empty and sub-cadence sorts as well.
pub(crate) const FILE_SORT_CONTROL_POLL_WORK_UNITS_V1: u64 = 128;

#[derive(Clone, Copy)]
pub(crate) enum FileSortEventV1 {
    Comparison,
    RecordRead,
    RecordWrite,
}

/// Checked work admission and observation for one in-place file-backed
/// heapsort plus one canonical-order scan. The budget is a conservative
/// derivation from the record count and heapsort height; exhausting it fails
/// closed rather than selecting another algorithm or allocating a larger
/// userspace structure.
pub(crate) struct FileSortWorkV1 {
    budget: u64,
    work_units: u64,
    next_poll: u64,
}

impl FileSortWorkV1 {
    pub(crate) fn begin<C: OperationWorkControlV1 + ?Sized>(
        record_count: u32,
        control: &mut C,
        counters: &mut OperationCountersV1,
    ) -> CoreResult<Self> {
        let count = u64::from(record_count);
        let levels = if record_count <= 1 {
            1
        } else {
            u64::from(u32::BITS - (record_count - 1).leading_zeros())
        };
        let sift_calls = count
            .checked_div(2)
            .and_then(|calls| calls.checked_add(count.saturating_sub(1)))
            .ok_or(CoreError::IntegerOverflow)?;
        // One sift level performs at most three record reads, two
        // comparisons, and two record writes. Extraction adds two reads and
        // two writes, while the canonical scan is allowed four events per
        // record. Sixty-four fixed units cover phase bookkeeping.
        let budget = sift_calls
            .checked_mul(levels)
            .and_then(|units| units.checked_mul(7))
            .and_then(|units| {
                count
                    .saturating_sub(1)
                    .checked_mul(4)
                    .and_then(|extract| units.checked_add(extract))
            })
            .and_then(|units| {
                count
                    .checked_mul(4)
                    .and_then(|scan| units.checked_add(scan))
            })
            .and_then(|units| units.checked_add(64))
            .ok_or(CoreError::IntegerOverflow)?;
        counters.file_sort_maximum_work_budget = counters.file_sort_maximum_work_budget.max(budget);
        // The current implementations sort in place in the already charged
        // spool and create no auxiliary run or temporary file. This direct
        // observed high-water therefore remains at its existing value (zero
        // for a fresh operation), not an unavailable physical observation.
        let mut work = Self {
            budget,
            work_units: 0,
            next_poll: FILE_SORT_CONTROL_POLL_WORK_UNITS_V1,
        };
        work.poll(control, counters)?;
        Ok(work)
    }

    pub(crate) fn begin_event<C: OperationWorkControlV1 + ?Sized>(
        &mut self,
        event: FileSortEventV1,
        control: &mut C,
        counters: &mut OperationCountersV1,
    ) -> CoreResult<()> {
        let next = self
            .work_units
            .checked_add(1)
            .ok_or(CoreError::IntegerOverflow)?;
        if next > self.budget {
            return Err(CoreError::ResourceRefused);
        }
        if next >= self.next_poll {
            self.poll(control, counters)?;
            self.next_poll = self
                .next_poll
                .checked_add(FILE_SORT_CONTROL_POLL_WORK_UNITS_V1)
                .ok_or(CoreError::IntegerOverflow)?;
        }
        counters.record_file_sort_event_v1(event)?;
        self.work_units = next;
        Ok(())
    }

    pub(crate) fn begin_pass(&mut self, counters: &mut OperationCountersV1) -> CoreResult<()> {
        counters.record_file_sort_pass_v1()
    }

    pub(crate) fn finish<C: OperationWorkControlV1 + ?Sized>(
        &mut self,
        control: &mut C,
        counters: &mut OperationCountersV1,
    ) -> CoreResult<()> {
        self.poll(control, counters)
    }

    fn poll<C: OperationWorkControlV1 + ?Sized>(
        &mut self,
        control: &mut C,
        counters: &mut OperationCountersV1,
    ) -> CoreResult<()> {
        counters.record_file_sort_control_poll_v1()?;
        if control.cancellation_requested_v1() {
            Err(CoreError::Cancelled)
        } else if control.deadline_exceeded_v1() {
            Err(CoreError::Deadline)
        } else {
            Ok(())
        }
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
    PackLocalObjectsCreated,
    PackLocalObjectsReused,
    PhysicalCarrierObjectWrites,
    ClosureObjectsMissing,
    ClosureObjectsOccupiedValidated,
    TreeNodesCreated,
    TreeNodesReused,
    PackEntries,
    PackBytes,
    CarrierBytesTotal,
    CarrierRollovers,
    InstalledCarrierLogicalBytes,
    TemporaryPreparationBytes,
    GlobalSeenLookups,
    GlobalSeenProbes,
    GlobalSeenMetadataBytesRead,
    GlobalSeenMetadataReadCalls,
    GlobalSeenMetadataBytesWritten,
    GlobalSeenSameCarrierReuses,
    GlobalSeenCrossCarrierReuses,
    FsCasBytesRead,
    FsCasReadCalls,
    FsCasBytesWritten,
    FsCasCatalogOperations,
    LocatorInstalls,
    LocatorEqualIncumbentReuses,
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
    RetryAttempts,
    Redispatches,
    AutomaticFallbacks,
    ProviderSwitches,
    CdcSwitches,
    PublicationAuthorityDispatches,
    UpdateToReplaceFallbacks,
    FullBasePayloadFallbacks,
    FileSyncCalls,
    DirectorySyncCalls,
    WalOrRecoveryOperations,
    MemoryBackendOperations,
    WholePackCopies,
    FilesystemCloneReflinkOperations,
    LayerFsCreatedThreads,
    RayonWorkUnits,
    SourceSizedStagingAllocations,
    WorkspaceSizedStagingAllocations,
}

#[cfg(test)]
mod tests {
    use crate::cdc::{CdcStreamCountersV1, SeqCdcCountersV1};
    use crate::format::PhysicalObjectKindV1;

    use super::{CoreError, ObservationScopeV1, OperationCountersV1, OptionalU64ObservationV1};

    #[test]
    fn optional_observations_reject_empty_method_or_reason() {
        let operation = ObservationScopeV1::Operation;
        assert!(std::panic::catch_unwind(|| {
            OptionalU64ObservationV1::observed(1, "", operation)
        })
        .is_err());
        assert!(std::panic::catch_unwind(|| {
            OptionalU64ObservationV1::unavailable("", operation)
        })
        .is_err());
        assert!(std::panic::catch_unwind(|| {
            OptionalU64ObservationV1::not_applicable("", operation)
        })
        .is_err());
        assert!(
            std::panic::catch_unwind(|| { OptionalU64ObservationV1::deferred("", operation) })
                .is_err()
        );
    }

    #[test]
    fn cdc_stream_accumulation_is_transactional_on_late_overflow() {
        let mut destination = OperationCountersV1 {
            ring_fills: 7,
            ring_wrap_spans: 11,
            cdc_scan_calls: 13,
            cdc_scan_bytes: 17,
            bytes_boundary_inspected: u64::MAX,
            ..OperationCountersV1::default()
        };
        let before = destination;

        assert_eq!(
            destination.add_cdc_stream(CdcStreamCountersV1 {
                ring_fills: 1,
                ring_wrap_spans: 2,
                scan_calls: 3,
                scan_bytes: 4,
                boundary_inspected_bytes: 1,
            }),
            Err(CoreError::IntegerOverflow)
        );
        assert_eq!(destination, before);
    }

    #[test]
    fn seqcdc_accumulation_is_transactional_on_late_overflow() {
        let mut destination = OperationCountersV1 {
            seqcdc_comparisons: 7,
            seqcdc_equal_absorptions: 11,
            seqcdc_opposing_slopes: 13,
            seqcdc_jumps: 17,
            seqcdc_jump_bytes: u64::MAX,
            ..OperationCountersV1::default()
        };
        let before = destination;

        assert_eq!(
            destination.add_seqcdc(SeqCdcCountersV1 {
                comparisons: 1,
                equal_absorptions: 2,
                opposing_slopes: 3,
                jumps: 4,
                jump_bytes: 5,
            }),
            Err(CoreError::IntegerOverflow)
        );
        assert_eq!(destination, before);
    }

    #[test]
    fn pack_storage_accumulation_is_transactional_on_late_overflow() {
        let mut destination = OperationCountersV1 {
            installed_carrier_logical_bytes: 7,
            temporary_preparation_bytes: u64::MAX,
            pack_entries: 11,
            ..OperationCountersV1::default()
        };
        let before = destination;

        assert_eq!(
            destination.record_pack_storage(13, 17),
            Err(CoreError::IntegerOverflow)
        );
        assert_eq!(destination, before);
    }

    #[test]
    fn carrier_accumulation_is_transactional_on_late_overflow() {
        let mut destination = OperationCountersV1 {
            carrier_bytes_total: 7,
            final_carrier_bytes: 11,
            maximum_active_carrier_bytes: 13,
            carrier_rollovers: u64::MAX,
            pack_entries: 17,
            ..OperationCountersV1::default()
        };
        let before = destination;

        assert_eq!(
            destination.record_carrier(19, true),
            Err(CoreError::IntegerOverflow)
        );
        assert_eq!(destination, before);
    }

    #[test]
    fn carrier_accumulation_records_exact_totals_final_maximum_and_rollovers() {
        let mut destination = OperationCountersV1::default();

        destination.record_carrier(19, false).unwrap();
        destination.record_carrier(7, true).unwrap();
        destination.record_carrier(23, true).unwrap();

        assert_eq!(destination.carrier_bytes_total, 49);
        assert_eq!(destination.final_carrier_bytes, 23);
        assert_eq!(destination.maximum_active_carrier_bytes, 23);
        assert_eq!(destination.carrier_rollovers, 2);
    }

    #[test]
    fn global_seen_accumulation_is_transactional_on_late_overflow() {
        let mut destination = OperationCountersV1 {
            global_seen_lookups: 7,
            global_seen_probes: 11,
            global_seen_metadata_bytes_read: 13,
            global_seen_metadata_read_calls: 17,
            global_seen_metadata_bytes_written: u64::MAX,
            global_seen_maximum_probe: 19,
            global_seen_entries: 23,
            global_seen_table_bytes: 29,
            pack_entries: 31,
            ..OperationCountersV1::default()
        };
        let before = destination;

        assert_eq!(
            destination.record_global_seen(37, 41, 43, 47, 53, 59, 61, 67),
            Err(CoreError::IntegerOverflow)
        );
        assert_eq!(destination, before);
    }

    #[test]
    fn created_object_disposition_is_transactional_on_late_kind_overflow() {
        let mut destination = OperationCountersV1 {
            pack_local_objects_created: 7,
            physical_carrier_object_writes: 11,
            chunk_objects_created: u64::MAX,
            pack_entries: 13,
            ..OperationCountersV1::default()
        };
        let before = destination;

        assert_eq!(
            destination.record_pack_object_disposition(PhysicalObjectKindV1::Chunk, true),
            Err(CoreError::IntegerOverflow)
        );
        assert_eq!(destination, before);
    }

    #[test]
    fn reused_object_disposition_is_transactional_on_late_kind_overflow() {
        let mut destination = OperationCountersV1 {
            pack_local_objects_reused: 7,
            file_objects_reused: u64::MAX,
            pack_entries: 11,
            ..OperationCountersV1::default()
        };
        let before = destination;

        assert_eq!(
            destination.record_pack_object_disposition(PhysicalObjectKindV1::File, false),
            Err(CoreError::IntegerOverflow)
        );
        assert_eq!(destination, before);
    }

    #[test]
    fn counter_accumulation_is_transactional_on_release_counter_overflow() {
        let mut destination = OperationCountersV1 {
            bytes_read: 7,
            root_admission_release_failures: u64::MAX,
            maximum_active_carrier_bytes: 11,
            storage_preparation_bytes_current_after_cleanup: 13,
            ..OperationCountersV1::default()
        };
        let source = OperationCountersV1 {
            bytes_read: 1,
            root_admission_release_failures: 1,
            maximum_active_carrier_bytes: 99,
            storage_preparation_bytes_current_after_cleanup: 17,
            ..OperationCountersV1::default()
        };
        let before = destination;

        assert_eq!(
            destination.accumulate(source),
            Err(CoreError::IntegerOverflow)
        );
        assert_eq!(
            destination.root_admission_release_failure_observation_error_v1(),
            Some(CoreError::IntegerOverflow)
        );
        destination.root_admission_release_failure_observation_error =
            before.root_admission_release_failure_observation_error;
        assert_eq!(destination, before);
    }

    #[test]
    fn counter_accumulation_is_transactional_on_preparation_current_overflow() {
        let mut destination = OperationCountersV1 {
            bytes_read: 7,
            root_admission_release_failures: 3,
            maximum_active_carrier_bytes: 11,
            storage_preparation_bytes_current_after_cleanup: u64::MAX,
            ..OperationCountersV1::default()
        };
        let source = OperationCountersV1 {
            bytes_read: 1,
            root_admission_release_failures: 5,
            maximum_active_carrier_bytes: 99,
            storage_preparation_bytes_current_after_cleanup: 1,
            ..OperationCountersV1::default()
        };
        let before = destination;

        assert_eq!(
            destination.accumulate(source),
            Err(CoreError::IntegerOverflow)
        );
        assert_eq!(destination, before);
    }

    #[test]
    fn incumbent_comparison_is_transactional_on_late_windows_overflow() {
        let mut destination = OperationCountersV1 {
            incumbent_comparison_bytes: 7,
            incumbent_comparison_windows: u64::MAX,
            fscas_bytes_read: 11,
            ..OperationCountersV1::default()
        };
        let before = destination;

        assert_eq!(
            destination.record_incumbent_comparison(13, 1),
            Err(CoreError::IntegerOverflow)
        );
        assert_eq!(destination, before);
    }

    #[test]
    fn storage_admission_is_transactional_on_late_residue_overflow() {
        let mut destination = OperationCountersV1 {
            storage_bytes_requested: 7,
            storage_bytes_reserved: 11,
            storage_bytes_released: 13,
            storage_bytes_committed: 17,
            storage_bytes_retained: 19,
            storage_inodes_requested: 23,
            storage_preparation_bytes_high_water: 29,
            immutable_residue_inodes: u64::MAX,
            ..OperationCountersV1::default()
        };
        let before = destination;

        assert_eq!(
            destination.record_storage_admission_v1(
                1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 31, 37, 41, 43, 47, 53, 59, 61, 67, 1,
            ),
            Err(CoreError::IntegerOverflow)
        );
        assert_eq!(destination, before);
    }

    #[test]
    fn update_reference_metadata_is_transactional_on_late_byte_overflow() {
        let mut destination = OperationCountersV1 {
            update_reference_metadata_records: 7,
            update_reference_metadata_bytes: u64::MAX,
            update_base_payload_bytes: 11,
            source_bytes_read: 13,
            ..OperationCountersV1::default()
        };
        let before = destination;

        assert_eq!(
            destination.record_update_reference_metadata(1, 36),
            Err(CoreError::IntegerOverflow)
        );
        assert_eq!(destination, before);
    }

    #[test]
    fn exact_rejoin_success_is_transactional_on_late_outcome_overflow() {
        let mut destination = OperationCountersV1 {
            exact_rejoin_bytes: 7,
            rejoin_successes: u64::MAX,
            rejoin_failures: 11,
            update_base_payload_bytes: 13,
            ..OperationCountersV1::default()
        };
        let before = destination;

        assert_eq!(
            destination.record_exact_rejoin(17, true),
            Err(CoreError::IntegerOverflow)
        );
        assert_eq!(destination, before);
    }

    #[test]
    fn exact_rejoin_failure_is_transactional_on_late_outcome_overflow() {
        let mut destination = OperationCountersV1 {
            exact_rejoin_bytes: 7,
            rejoin_successes: 11,
            rejoin_failures: u64::MAX,
            update_base_payload_bytes: 13,
            ..OperationCountersV1::default()
        };
        let before = destination;

        assert_eq!(
            destination.record_exact_rejoin(17, false),
            Err(CoreError::IntegerOverflow)
        );
        assert_eq!(destination, before);
    }
}
