//! Narrow authenticated canonical-object provider.
//!
//! C1 never opens a database, a pack or a file. It asks its caller for canonical
//! bytes and receives them back; the provider contract is that returned bytes are
//! already authenticated against the requested identity. A batch is a real
//! grouped demand, not a loop of point calls, and it returns values in demand
//! order with exact cardinality.

use crate::error::{ContentError, ContentResult};
use crate::object::ObjectId;

/// Reads authenticated canonical objects for a construction or a logical read.
///
/// Implementations must return the canonical object bytes whose identity is
/// `ids[index]`, in that order. Returning bytes that do not belong to the
/// requested identity is a contract violation; a provider that cannot establish
/// identity must return [`ContentError::IdentityMismatch`] instead.
pub trait AuthenticatedObjects {
    /// Reads every requested object as one bounded grouped demand.
    fn read_canonical_batch(&self, ids: &[ObjectId]) -> ContentResult<Vec<Vec<u8>>>;

    /// Reads one object; a point read is a one-ID batch.
    fn read_canonical(&self, id: ObjectId) -> ContentResult<Vec<u8>> {
        let mut values = self.read_canonical_batch(std::slice::from_ref(&id))?;
        if values.len() != 1 {
            return Err(ContentError::BatchCardinality {
                requested: 1,
                returned: values.len(),
            });
        }
        values.pop().ok_or(ContentError::BatchCardinality {
            requested: 1,
            returned: 0,
        })
    }
}
