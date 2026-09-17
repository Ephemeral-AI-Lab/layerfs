//! Narrow authenticated canonical-object provider.
//!
//! C1 never opens a database, a pack or a file. It asks its caller for canonical
//! bytes and receives them back; the provider contract is that returned bytes are
//! already authenticated against the requested identity. A batch is a real
//! grouped demand, not a loop of point calls, and it returns values in demand
//! order with exact cardinality.
//!
//! A demand may carry the caller's timing scope. The scope is optional by
//! construction: [`AuthenticatedObjects::read_canonical_batch`] stays the one
//! required method, and [`AuthenticatedObjects::read_canonical_batch_scoped`]
//! forwards to it by default. A provider that only serves bytes from memory
//! implements nothing extra; a provider whose demand performs real work of its own
//! overrides the scoped form, so that work is a named child of the caller's span
//! instead of being merged into the caller's own duration. The Store bridge in
//! `layerfs-storage` is exactly such a provider.

use layerfs_telemetry::timer::TimingScope;

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

    /// Reads every requested object under the caller's timing scope.
    ///
    /// `scope` is a planned node of the caller's operation tree. A provider that
    /// performs separable work starts it (`scope.run`) and nests its own children
    /// inside; the default implementation forwards to
    /// [`Self::read_canonical_batch`] and records nothing, which is the honest
    /// answer for a provider that only hands back bytes it already holds.
    fn read_canonical_batch_scoped(
        &self,
        ids: &[ObjectId],
        scope: TimingScope<'_>,
    ) -> ContentResult<Vec<Vec<u8>>> {
        let _ = scope;
        self.read_canonical_batch(ids)
    }

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

    /// Reads one object under the caller's timing scope.
    fn read_canonical_scoped(
        &self,
        id: ObjectId,
        scope: TimingScope<'_>,
    ) -> ContentResult<Vec<u8>> {
        let mut values = self.read_canonical_batch_scoped(std::slice::from_ref(&id), scope)?;
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
