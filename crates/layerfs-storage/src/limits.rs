//! Fixed L1 memory ledger and proof counters.

use core::sync::atomic::{AtomicU64, Ordering};

use crate::{CoreError, CoreResult};

pub const BASE_LEDGER_BYTES: u64 = 8_388_608;
pub const OPERATION_SLOT_BYTES: u64 = 4_194_304;
pub const MEMORY_PROFILE_32_MIB: u64 = 32 * 1_024 * 1_024;
pub const MEMORY_PROFILE_48_MIB: u64 = 48 * 1_024 * 1_024;
pub const MEMORY_PROFILE_72_MIB: u64 = 72 * 1_024 * 1_024;

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
}

impl OperationMemoryPlanV1 {
    pub const fn empty() -> Self {
        Self {
            components: 0,
            total_bytes: 0,
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
        Ok(self)
    }

    pub const fn total_bytes(self) -> u64 {
        self.total_bytes
    }

    pub const fn contains(self, component: MemoryComponentV1) -> bool {
        self.components & (1_u16 << component as u8) != 0
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct OperationCountersV1 {
    pub bytes_read: u64,
    pub bytes_written: u64,
    pub bytes_copied: u64,
    pub bytes_structurally_reused: u64,
    pub logical_chunks_created: u64,
    pub logical_chunks_reused: u64,
    pub physical_objects_created: u64,
    pub physical_objects_reused: u64,
    pub tree_nodes_created: u64,
    pub tree_nodes_reused: u64,
    pub pack_entries: u64,
    pub pack_bytes: u64,
    pub update_resynchronization_bytes: u64,
    pub anchor_attempts: u64,
    pub update_failures: u64,
    pub fallback_attempts: u64,
    pub retries_or_redispatches: u64,
    pub publication_dispatches: u64,
    pub memory_high_water: u64,
}

impl OperationCountersV1 {
    pub fn record_fallback_attempt(&mut self) -> CoreResult<()> {
        self.add(CounterFieldV1::FallbackAttempts, 1)
    }

    pub fn record_retry_or_redispatch(&mut self) -> CoreResult<()> {
        self.add(CounterFieldV1::RetriesOrRedispatches, 1)
    }

    pub fn record_publication_dispatch(&mut self) -> CoreResult<()> {
        self.add(CounterFieldV1::PublicationDispatches, 1)
    }

    pub(crate) fn add(&mut self, field: CounterFieldV1, amount: u64) -> CoreResult<()> {
        let target = match field {
            CounterFieldV1::BytesRead => &mut self.bytes_read,
            CounterFieldV1::BytesWritten => &mut self.bytes_written,
            CounterFieldV1::BytesCopied => &mut self.bytes_copied,
            CounterFieldV1::BytesStructurallyReused => &mut self.bytes_structurally_reused,
            CounterFieldV1::LogicalChunksCreated => &mut self.logical_chunks_created,
            CounterFieldV1::LogicalChunksReused => &mut self.logical_chunks_reused,
            CounterFieldV1::PhysicalObjectsCreated => &mut self.physical_objects_created,
            CounterFieldV1::PhysicalObjectsReused => &mut self.physical_objects_reused,
            CounterFieldV1::TreeNodesCreated => &mut self.tree_nodes_created,
            CounterFieldV1::TreeNodesReused => &mut self.tree_nodes_reused,
            CounterFieldV1::PackEntries => &mut self.pack_entries,
            CounterFieldV1::PackBytes => &mut self.pack_bytes,
            CounterFieldV1::UpdateResynchronizationBytes => {
                &mut self.update_resynchronization_bytes
            }
            CounterFieldV1::AnchorAttempts => &mut self.anchor_attempts,
            CounterFieldV1::UpdateFailures => &mut self.update_failures,
            CounterFieldV1::FallbackAttempts => &mut self.fallback_attempts,
            CounterFieldV1::RetriesOrRedispatches => &mut self.retries_or_redispatches,
            CounterFieldV1::PublicationDispatches => &mut self.publication_dispatches,
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
    BytesWritten,
    BytesCopied,
    BytesStructurallyReused,
    LogicalChunksCreated,
    LogicalChunksReused,
    PhysicalObjectsCreated,
    PhysicalObjectsReused,
    TreeNodesCreated,
    TreeNodesReused,
    PackEntries,
    PackBytes,
    UpdateResynchronizationBytes,
    AnchorAttempts,
    UpdateFailures,
    FallbackAttempts,
    RetriesOrRedispatches,
    PublicationDispatches,
}
