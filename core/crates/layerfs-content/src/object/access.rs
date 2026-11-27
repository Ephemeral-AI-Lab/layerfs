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
/// identity must return [`ContentError::IdentityMismatch`] instead. Absence is
/// reported as [`ContentError::MissingObject`] and nothing else: a provider
/// that holds state for the request but cannot serve it - corrupt storage, a
/// record outside this reader's visibility, a capacity refusal - reports
/// [`ContentError::ProviderFailure`] so the two classes stay distinguishable.
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

    /// Already authenticated bytes this owner retains, in request order.
    /// An absent entry is an ordinary cache miss. The answer performs no
    /// provider operation and charges no more than `bytes` canonical bytes.
    fn cached_canonical_batch(
        &self,
        ids: &[ObjectId],
        _bytes: usize,
    ) -> ContentResult<Vec<Option<Vec<u8>>>> {
        Ok(ids.iter().map(|_| None).collect())
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

    /// Reads every requested object, each into its own result, in request
    /// order (#818 H5). A missing, mismatched or undecodable object is that
    /// object's error, so a caller that reads ahead (a window of versions)
    /// meets it only where it reaches that object, as it would reading the
    /// object alone. A failure the provider cannot tie to one object (its
    /// session, its transport, a demand over its bounds) fails the call.
    ///
    /// This default reads the objects one by one, each a point read, so each
    /// read's failure is its own object's; no object is read twice and no
    /// failed read is attempted again. A provider that serves the objects in
    /// one demand overrides it and says so in [`Self::reads_each_at_once`].
    fn read_canonical_each(&self, ids: &[ObjectId]) -> ContentResult<Vec<ContentResult<Vec<u8>>>> {
        Ok(ids.iter().map(|id| self.read_canonical(*id)).collect())
    }

    /// Whether [`Self::read_canonical_each`] serves its objects in one
    /// demand. The default reads them one by one: a caller batches across
    /// versions only where this is true.
    fn reads_each_at_once(&self) -> bool {
        false
    }

    /// Decoded pages this provider keeps for reuse (#364): a read that
    /// finds a page there skips its copy and decode, never a check that
    /// depends on where the page was found. None keeps nothing.
    fn decoded_pages(&self) -> Option<&crate::filesystem::tree::pages::decoded::DecodedPages> {
        None
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    /// Serves `present`; any other object is missing. Records each demand.
    struct Objects {
        present: Vec<ObjectId>,
        demands: RefCell<Vec<Vec<ObjectId>>>,
    }

    impl AuthenticatedObjects for Objects {
        fn read_canonical_batch(&self, ids: &[ObjectId]) -> ContentResult<Vec<Vec<u8>>> {
            self.demands.borrow_mut().push(ids.to_vec());
            ids.iter()
                .map(|id| {
                    if self.present.contains(id) {
                        Ok(id.as_bytes().to_vec())
                    } else {
                        Err(ContentError::MissingObject)
                    }
                })
                .collect()
        }
    }

    /// The default reads each object alone: a missing one is its own error,
    /// the others are served, and nothing is read twice.
    #[test]
    fn each_object_is_its_own_result() {
        let ids: Vec<ObjectId> = (0..4u8).map(|n| ObjectId::for_bytes(&[n])).collect();
        let objects = Objects {
            present: vec![ids[0], ids[1], ids[3]],
            demands: RefCell::new(Vec::new()),
        };
        let each = objects.read_canonical_each(&ids).unwrap();
        assert_eq!(each[0].as_deref().unwrap(), ids[0].as_bytes());
        assert_eq!(each[2], Err(ContentError::MissingObject));
        assert_eq!(each[3].as_deref().unwrap(), ids[3].as_bytes());
        let demands = objects.demands.into_inner();
        assert_eq!(demands, ids.iter().map(|id| vec![*id]).collect::<Vec<_>>());
        assert!(!objects_at_once());
    }

    fn objects_at_once() -> bool {
        Objects {
            present: Vec::new(),
            demands: RefCell::new(Vec::new()),
        }
        .reads_each_at_once()
    }
}
