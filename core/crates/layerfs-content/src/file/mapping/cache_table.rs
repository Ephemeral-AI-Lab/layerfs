//! Scoped fixed mapping-table metadata, separate from canonical output data.
use crate::object::{CanonicalBuffer, ObjectId};
use crate::{ContentError, ContentResult};
use std::alloc::Layout;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
/// Existing cache metadata allowance; not a native/process guard.
pub const READ_NAVIGATION_CACHE_METADATA_BYTES: usize = 16 * 1024;
/// Exact compiled requested HashMap table layout for the pinned Rust profile.
#[derive(Clone, Copy, Debug)]
pub struct CacheTableLayout {
    /// Actual compiled key/leased-value entry Layout width.
    pub entry_bytes: usize,
    /// Exact declared table bucket count.
    pub buckets: usize,
    /// Actual control group width of the supported pinned architecture.
    pub group_bytes: usize,
    /// Exact requested table allocation, including aligned controls.
    pub allocation_bytes: usize,
    /// Expected effective HashMap capacity after a single reserve.
    pub capacity: usize,
}
impl CacheTableLayout {
    pub(crate) fn new(count: usize) -> ContentResult<Self> {
        let overflow = || ContentError::ResourceUnavailable {
            what: "mapping.cache_layout",
        };
        let buckets = if count < 4 {
            4
        } else if count < 8 {
            8
        } else {
            count
                .checked_mul(8)
                .ok_or_else(overflow)?
                .checked_div(7)
                .ok_or_else(overflow)?
                .checked_next_power_of_two()
                .ok_or_else(overflow)?
        };
        let entry = Layout::new::<((ObjectId, bool), CanonicalBuffer)>();
        let group_bytes = if cfg!(all(
            target_feature = "sse2",
            any(target_arch = "x86", target_arch = "x86_64")
        )) {
            16
        } else {
            std::mem::size_of::<usize>()
        };
        let align = entry.align().max(group_bytes);
        let offset = entry
            .size()
            .checked_mul(buckets)
            .and_then(|n| n.checked_add(align - 1))
            .ok_or_else(overflow)?
            & !(align - 1);
        let size = offset
            .checked_add(buckets)
            .and_then(|n| n.checked_add(group_bytes))
            .ok_or_else(overflow)?;
        let layout = Layout::from_size_align(size, align).map_err(|_| overflow())?;
        Ok(Self {
            entry_bytes: entry.size(),
            buckets,
            group_bytes,
            allocation_bytes: layout.size(),
            capacity: if buckets < 8 {
                buckets - 1
            } else {
                buckets / 8 * 7
            },
        })
    }
}
/// One continuing metadata authority; no canonical Vec data is charged here.
#[derive(Clone)]
pub struct CacheTableMemory(Arc<AtomicUsize>);
impl CacheTableMemory {
    pub(crate) fn new() -> Self {
        Self(Arc::new(AtomicUsize::new(Self::control_bytes())))
    }
    /// Exact compiled Arc header and numeric controller allocation.
    pub const fn control_bytes() -> usize {
        2 * std::mem::size_of::<usize>() + std::mem::size_of::<AtomicUsize>()
    }
    /// Actual retained table grant plus live controller allocation.
    pub fn reserved_bytes(&self) -> usize {
        self.0.load(Ordering::Acquire)
    }
    pub(crate) fn reserve(&self, bytes: usize) -> ContentResult<CacheTableLease> {
        self.0
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |used| {
                used.checked_add(bytes)
                    .filter(|n| *n <= READ_NAVIGATION_CACHE_METADATA_BYTES)
            })
            .map_err(|_| ContentError::BoundedCapacityExceeded {
                what: "mapping.cache_metadata",
                limit: READ_NAVIGATION_CACHE_METADATA_BYTES as u64,
                actual: self.reserved_bytes().saturating_add(bytes) as u64,
            })?;
        Ok(CacheTableLease {
            memory: self.clone(),
            bytes,
        })
    }
}
pub(crate) struct CacheTableLease {
    memory: CacheTableMemory,
    bytes: usize,
}
impl Drop for CacheTableLease {
    fn drop(&mut self) {
        self.memory.0.fetch_sub(self.bytes, Ordering::AcqRel);
    }
}
