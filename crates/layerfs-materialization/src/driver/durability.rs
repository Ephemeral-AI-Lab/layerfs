//! Projection durability classification.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DurabilityClass {
    ProcessCrashReconciled,
    HostCrashOrdered,
    DeviceFlushRequested,
    PowerLossQualified,
}

/// Directory durability for one atomic install. Deferral is valid only while
/// building a fresh tree that cannot become Complete until later bottom-up
/// directory barriers and root revalidation succeed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DirectoryDurability {
    ImmediateDirectoryDurability,
    DeferredToIncompleteTreeBoundary,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DurabilityClassCounts {
    pub process_crash_reconciled: u64,
    pub host_crash_ordered: u64,
    pub device_flush_requested: u64,
    pub power_loss_qualified: u64,
}

impl DurabilityClassCounts {
    pub fn get(&self, class: DurabilityClass) -> u64 {
        match class {
            DurabilityClass::ProcessCrashReconciled => self.process_crash_reconciled,
            DurabilityClass::HostCrashOrdered => self.host_crash_ordered,
            DurabilityClass::DeviceFlushRequested => self.device_flush_requested,
            DurabilityClass::PowerLossQualified => self.power_loss_qualified,
        }
    }

    pub(super) fn checked_delta(self, before: Self) -> Option<Self> {
        Some(Self {
            process_crash_reconciled: self
                .process_crash_reconciled
                .checked_sub(before.process_crash_reconciled)?,
            host_crash_ordered: self
                .host_crash_ordered
                .checked_sub(before.host_crash_ordered)?,
            device_flush_requested: self
                .device_flush_requested
                .checked_sub(before.device_flush_requested)?,
            power_loss_qualified: self
                .power_loss_qualified
                .checked_sub(before.power_loss_qualified)?,
        })
    }

    pub(super) fn checked_add(self, other: Self) -> Option<Self> {
        Some(Self {
            process_crash_reconciled: self
                .process_crash_reconciled
                .checked_add(other.process_crash_reconciled)?,
            host_crash_ordered: self
                .host_crash_ordered
                .checked_add(other.host_crash_ordered)?,
            device_flush_requested: self
                .device_flush_requested
                .checked_add(other.device_flush_requested)?,
            power_loss_qualified: self
                .power_loss_qualified
                .checked_add(other.power_loss_qualified)?,
        })
    }
}
