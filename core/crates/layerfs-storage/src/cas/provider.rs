//! The one product bridge from a persisted Store to C1's provider contract.
//!
//! C1 asks its caller for authenticated canonical bytes and never opens a
//! database, a pack or a file. A runtime adapter that owns a Store and drives a
//! C1 component therefore needs exactly one object between them, and this is it:
//! it reads through the Store's own bounded read wave, so the wave ceiling, the
//! pack ceiling and the decode workspace stay the Store's, and it reinterprets
//! nothing - the values it returns are the canonical bytes the Store
//! reconstructed and authenticated.

use layerfs_content::object::{AuthenticatedObjects, ObjectId};
use layerfs_content::{ContentError, ContentResult};
use layerfs_telemetry::timer::{Timing, TimingScope};

use crate::cas::store::{Store, StoreReadCounters};
use crate::error::{StorageError, StorageResult};

/// Maps one Store read failure onto the provider contract's error classes.
///
/// `MissingObject` is the answer for absence and for nothing else, because a
/// caller-authorized value root is allowed to name an object this Store does
/// not hold (`admission-and-persistence.md`). Every other failure - a corrupt
/// locator or pack, a record above the publication watermark, a capacity
/// refusal, an engine failure - reaches C1 as a distinguishable
/// [`ContentError::ProviderFailure`] instead of masquerading as absence, which
/// is the distinction a later adapter needs between "this root is not in this
/// Store" and "this Store is corrupt".
fn provider_error(error: StorageError) -> ContentError {
    match error {
        StorageError::ObjectMissing(_) => ContentError::MissingObject,
        StorageError::Content(content) => content,
        StorageError::Integrity(what) => ContentError::ProviderFailure { what },
        StorageError::CapacityExceeded { what, .. } => ContentError::ProviderFailure { what },
        StorageError::UnsupportedPolicy { field } => ContentError::ProviderFailure { what: field },
        StorageError::VisibilityCeiling { .. } => ContentError::ProviderFailure {
            what: "record above the visibility ceiling",
        },
        StorageError::Collision(_) => ContentError::ProviderFailure {
            what: "identity collision",
        },
        StorageError::MissingDependency { .. } => ContentError::ProviderFailure {
            what: "missing dependency",
        },
        StorageError::Engine(_) => ContentError::ProviderFailure {
            what: "engine failure",
        },
        _ => ContentError::ProviderFailure {
            what: "store read failure",
        },
    }
}

/// A Store presented as C1's authenticated canonical-object provider.
pub struct StoreProvider<'a> {
    store: &'a Store,
}

impl<'a> StoreProvider<'a> {
    /// Wraps one Store as a provider.
    pub const fn new(store: &'a Store) -> Self {
        Self { store }
    }

    /// Reads one wave and returns the Store's own counters beside the values.
    ///
    /// This is the form an adapter with a timing tree of its own uses: the
    /// caller's scope becomes the wave's node, so the connection, the ceiling
    /// read and the decode are attributed inside the caller's span instead of
    /// being merged into the caller's own duration.
    pub fn read_wave(
        &self,
        ids: &[ObjectId],
        scope: TimingScope<'_>,
    ) -> StorageResult<(Vec<Vec<u8>>, StoreReadCounters)> {
        self.store.read_batch(ids, scope)
    }
}

impl AuthenticatedObjects for StoreProvider<'_> {
    /// Reads a wave under a disabled node.
    ///
    /// A provider call reached without an operation tree still performs the real
    /// read; there is simply nothing to attribute it to, and inventing a tree
    /// here would attribute the caller's demand to this layer.
    fn read_canonical_batch(&self, ids: &[ObjectId]) -> ContentResult<Vec<Vec<u8>>> {
        Timing::disabled("storage.read", |scope| {
            self.read_wave(ids, scope.child("storage.read"))
        })
        .0
        .map(|(values, _)| values)
        .map_err(provider_error)
    }

    /// Reads a wave as a child of the caller's span.
    fn read_canonical_batch_scoped(
        &self,
        ids: &[ObjectId],
        scope: TimingScope<'_>,
    ) -> ContentResult<Vec<Vec<u8>>> {
        self.read_wave(ids, scope)
            .map(|(values, _)| values)
            .map_err(provider_error)
    }
}
