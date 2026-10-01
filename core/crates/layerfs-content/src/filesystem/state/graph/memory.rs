//! Scoped graph working admission carried through real result ownership.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use crate::error::{ContentError, ContentResult};

/// Fixed logical graph state/attempt/result class; not encoded or native memory.
pub const GRAPH_WORKING_BYTES: usize = 65_536;

#[derive(Debug)]
struct Shared {
    reserved: AtomicUsize,
}

/// One graph owner's working class, independent of native scratch reservation.
#[derive(Clone, Debug)]
pub struct GraphMemory(Arc<Shared>);

impl GraphMemory {
    /// Charge the actual shared control layout before its allocation.
    pub fn new() -> Self {
        Self(Arc::new(Shared {
            reserved: AtomicUsize::new(Self::control_bytes()),
        }))
    }

    /// Shared allocator request: Arc strong/weak counters and its fixed state.
    pub const fn control_bytes() -> usize {
        2 * std::mem::size_of::<usize>() + std::mem::size_of::<Shared>()
    }

    /// Fixed simultaneous logical owner allowance.
    pub const fn limit(&self) -> usize {
        GRAPH_WORKING_BYTES
    }

    /// Bytes reserved by live state, attempts and consumer-held results.
    pub fn reserved_bytes(&self) -> usize {
        self.0.reserved.load(Ordering::Acquire)
    }

    /// Actual controller/lease/observer owners, used before known-clean rebind.
    /// This is a lifetime observation, not additional credit or a global RAM claim.
    pub fn handles(&self) -> usize {
        Arc::strong_count(&self.0)
    }

    /// Reserve a prospective allocation before acquiring or mutating its owner.
    pub fn reserve(&self, bytes: usize) -> ContentResult<GraphMemoryLease> {
        self.0
            .reserved
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |current| {
                current
                    .checked_add(bytes)
                    .filter(|total| *total <= GRAPH_WORKING_BYTES)
            })
            .map_err(|_| ContentError::ResourceUnavailable {
                what: "graph.working_memory",
            })?;
        Ok(GraphMemoryLease {
            memory: self.clone(),
            bytes,
        })
    }
}

impl Default for GraphMemory {
    fn default() -> Self {
        Self::new()
    }
}

/// Exclusive credit transferred with the allocation/result it funds.
/// Moving it preserves credit; it cannot be cloned or independently refunded.
#[derive(Debug)]
pub struct GraphMemoryLease {
    memory: GraphMemory,
    bytes: usize,
}

impl GraphMemoryLease {
    /// Exact requested layout allowance of this owner.
    pub const fn bytes(&self) -> usize {
        self.bytes
    }

    /// Borrow the live owner to reserve simultaneous dependent result capacity.
    pub fn memory(&self) -> &GraphMemory {
        &self.memory
    }

    /// Transfer exclusive prospective credit without a new reservation/refund.
    /// Parent and child retain one controller and exactly the prior combined bytes.
    pub fn split(&mut self, bytes: usize) -> ContentResult<Self> {
        if bytes == 0 || bytes > self.bytes {
            return Err(ContentError::BoundedCapacityExceeded {
                what: "graph.working_transfer",
                limit: self.bytes as u64,
                actual: bytes as u64,
            });
        }
        self.bytes -= bytes;
        Ok(Self {
            memory: self.memory.clone(),
            bytes,
        })
    }

    pub(super) fn check(&self, bytes: usize) -> ContentResult<()> {
        if bytes > self.bytes {
            return Err(ContentError::BoundedCapacityExceeded {
                what: "graph.working_lease",
                limit: self.bytes as u64,
                actual: bytes as u64,
            });
        }
        Ok(())
    }
}

impl Drop for GraphMemoryLease {
    fn drop(&mut self) {
        self.memory
            .0
            .reserved
            .fetch_sub(self.bytes, Ordering::AcqRel);
    }
}
