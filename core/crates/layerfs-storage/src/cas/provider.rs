//! The one product bridge from a persisted Store to C1's provider contract.
//!
//! C1 asks its caller for authenticated canonical bytes and never opens a
//! database, a pack or a file. A runtime adapter that owns a Store and drives a
//! C1 component therefore needs exactly one object between them, and this is it:
//! it reads through the Store's own bounded read wave, so the wave ceiling, the
//! pack ceiling and the decode workspace stay the Store's, and it reinterprets
//! nothing - the values it returns are the canonical bytes the Store
//! reconstructed and authenticated.

use std::cell::{Cell, RefCell};

use layerfs_content::object::{AuthenticatedObjects, ObjectId};
use layerfs_content::{ContentError, ContentResult};
use layerfs_telemetry::timer::{Timing, TimingScope};

use crate::cas::read::{check_read_demand, ReadSession};
use crate::cas::store::{Store, StoreReadCounters};
use crate::error::{StorageError, StorageResult};

/// Maps one Store read failure onto the provider contract's error classes.
///
/// `MissingObject` is the answer for absence and for nothing else, because a
/// caller-authorized value root is allowed to name an object this Store does
/// not hold (`admission-and-persistence.md`). Every other failure - a corrupt
/// locator or pack, a private or out-of-range record, a capacity
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
        StorageError::VisibilityCeiling { .. } | StorageError::Unpublished(_) => {
            ContentError::ProviderFailure {
                what: "record outside the publication scope",
            }
        }
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
///
/// The provider is one **operation's** adapter, and it carries that operation's
/// pooled read session: the first wave opens the connection and the decode arena,
/// every later wave reuses them, and the visibility ceiling is re-read per wave.
/// The session lives behind a [`RefCell`] in the provider and not in the [`Store`],
/// because the Store is shared and `Sync` while a session is one operation's
/// private state. The provider is therefore `!Sync`, which is the honest shape.
pub struct StoreProvider<'a> {
    store: &'a Store,
    /// The operation's session, opened by its first wave.
    session: RefCell<Option<ReadSession>>,
    /// Connections this provider's waves opened.
    ///
    /// A wave reports its own open through [`StoreReadCounters::opens`]; a caller
    /// that drives a whole operation through one provider reads the sum here
    /// instead of threading counters through every call.
    opens: Cell<u64>,
    /// Ordinary-lane group bodies every wave through this provider decompressed.
    ///
    /// The same shape as `opens`, for the same reason: a wave reports its own
    /// count, and an operation that issues many waves reads the sum here. It is
    /// the figure a decoded-group cache is measured against.
    group_decodes: Cell<u64>,
    pooled: Cell<crate::encoding::pool::PoolReadCounters>,
}

impl<'a> StoreProvider<'a> {
    /// Wraps one Store as a provider with no session yet.
    pub const fn new(store: &'a Store) -> Self {
        Self {
            store,
            session: RefCell::new(None),
            opens: Cell::new(0),
            group_decodes: Cell::new(0),
            pooled: Cell::new(crate::encoding::pool::PoolReadCounters::new()),
        }
    }

    /// Connections every wave this provider issued opened.
    pub fn connection_opens(&self) -> u64 {
        self.opens.get()
    }

    /// Ordinary-lane group bodies every wave this provider issued decompressed.
    pub fn group_decodes(&self) -> u64 {
        self.group_decodes.get()
    }

    /// Pooled reconstruction work over every successful wave of this operation.
    pub fn pooled_read_counters(&self) -> crate::encoding::pool::PoolReadCounters {
        self.pooled.get()
    }

    /// Reads one wave through the operation's session.
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
        scope.run(|read_scope| {
            // The wave's declared bound is checked before the session exists, so a
            // refused demand opens nothing.
            check_read_demand(ids, self.store.capacities().read_objects)?;
            let mut slot = self.session.borrow_mut();
            let opened = slot.is_none();
            if opened {
                *slot = Some(ReadSession::open(self.store.path())?);
            }
            let session = slot.as_mut().expect("the session was just opened");
            let (values, counters) = read_scope
                .child("storage.read")
                .run(|_| session.read(ids, &self.store.capacities()))?;
            if opened {
                self.opens.set(self.opens.get() + 1);
            }
            let mut pooled = self.pooled.get();
            pooled.accumulate(counters.pooled);
            self.pooled.set(pooled);
            self.group_decodes
                .set(self.group_decodes.get() + counters.group_decodes);
            Ok((
                values,
                StoreReadCounters {
                    objects: counters.objects,
                    packs_read: counters.packs_read,
                    pages: counters.pages,
                    ceiling: counters.ceiling,
                    edges: counters.edges,
                    max_depth: counters.max_depth,
                    canonical_bytes: counters.canonical_bytes,
                    group_decodes: counters.group_decodes,
                    pooled: counters.pooled,
                    opens: u64::from(opened),
                },
            ))
        })
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
