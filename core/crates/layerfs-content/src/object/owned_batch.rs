//! Immutable canonical buffers whose capacity credit follows actual data drop.

use super::{CanonicalBudget, CanonicalLease, CanonicalOwnership, CanonicalReadPermit};
use crate::error::{ContentError, ContentResult};

/// Immutable canonical data and its actual Vec-capacity lease.
/// No raw Vec conversion or implicit Clone can detach the admitted allocation.
pub struct CanonicalBuffer {
    bytes: Vec<u8>,
    ownership: CanonicalOwnership,
    // Last: data is freed before credit is returned.
    lease: CanonicalLease,
}
impl CanonicalBuffer {
    /// Explicit raw compatibility adoption; upstream allocation stays unqualified.
    pub fn compatibility(bytes: Vec<u8>, budget: &CanonicalBudget) -> ContentResult<Self> {
        let lease = budget.reserve(bytes.capacity())?;
        Ok(Self {
            bytes,
            ownership: CanonicalOwnership::Compatibility,
            lease,
        })
    }
    /// Immutable canonical bytes; the owner remains live through this borrow.
    pub fn as_slice(&self) -> &[u8] {
        &self.bytes
    }
    /// Actual canonical length.
    pub fn len(&self) -> usize {
        self.bytes.len()
    }
    /// Whether the actual canonical body is empty.
    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }
    /// Actual allocation capacity charged through this owner.
    pub fn capacity(&self) -> usize {
        self.bytes.capacity()
    }
    /// Exact originating budget, for a separately admitted retained copy.
    pub fn budget(&self) -> &CanonicalBudget {
        self.lease.budget()
    }
    /// Supplier ownership qualification; copying cannot improve it.
    pub fn ownership(&self) -> CanonicalOwnership {
        self.ownership
    }
    /// Reserve prospective copy capacity before allocating the immutable copy.
    pub fn try_clone(&self) -> ContentResult<Self> {
        let mut grant = self.budget().reserve(self.bytes.len())?;
        let mut bytes = Vec::new();
        bytes.try_reserve_exact(self.bytes.len()).map_err(|_| {
            ContentError::ResourceUnavailable {
                what: "canonical.copy_allocation",
            }
        })?;
        bytes.extend_from_slice(&self.bytes);
        let lease = grant.split(bytes.capacity())?;
        Ok(Self {
            bytes,
            ownership: self.ownership,
            lease,
        })
    }
}
impl std::ops::Deref for CanonicalBuffer {
    type Target = [u8];
    fn deref(&self) -> &[u8] {
        self.as_slice()
    }
}

/// Exact demanded owned results; descriptor/Vec-table memory is separate scope.
pub struct OwnedCanonicalBatch {
    buffers: Vec<CanonicalBuffer>,
    ownership: CanonicalOwnership,
}
struct SuppliedVectors {
    values: Vec<Vec<u8>>,
    permit: CanonicalReadPermit,
}
impl OwnedCanonicalBatch {
    /// Move producer allocations into their previously admitted output credit.
    /// Validate all actual capacities/cardinality before handing off any result.
    pub fn from_vectors(
        permit: CanonicalReadPermit,
        values: Vec<Vec<u8>>,
        ownership: CanonicalOwnership,
    ) -> ContentResult<Self> {
        let mut supplied = SuppliedVectors { values, permit };
        supplied.permit.check_count(supplied.values.len())?;
        let capacity = supplied
            .values
            .iter()
            .try_fold(0usize, |total, bytes| total.checked_add(bytes.capacity()))
            .ok_or(ContentError::LengthOverflow)?;
        if capacity > supplied.permit.maximum_capacity() {
            return Err(ContentError::BoundedCapacityExceeded {
                what: "canonical.returned_capacity",
                limit: supplied.permit.maximum_capacity() as u64,
                actual: capacity as u64,
            });
        }
        let mut buffers = Vec::new();
        buffers
            .try_reserve_exact(supplied.values.len())
            .map_err(|_| ContentError::ResourceUnavailable {
                what: "canonical.result_descriptors",
            })?;
        for bytes in std::mem::take(&mut supplied.values) {
            let lease = supplied.permit.lease.split(bytes.capacity())?;
            buffers.push(CanonicalBuffer {
                bytes,
                ownership,
                lease,
            });
        }
        Ok(Self { buffers, ownership })
    }
    /// Exact returned result count.
    pub fn len(&self) -> usize {
        self.buffers.len()
    }
    /// Whether the batch has no returned allocations.
    pub fn is_empty(&self) -> bool {
        self.buffers.is_empty()
    }
    /// Immutable owned result slice; no lease is detached.
    pub fn buffers(&self) -> &[CanonicalBuffer] {
        &self.buffers
    }
    /// Qualification selected by the real supplier.
    pub fn ownership(&self) -> CanonicalOwnership {
        self.ownership
    }
}
impl IntoIterator for OwnedCanonicalBatch {
    type Item = CanonicalBuffer;
    type IntoIter = std::vec::IntoIter<CanonicalBuffer>;
    fn into_iter(self) -> Self::IntoIter {
        self.buffers.into_iter()
    }
}
