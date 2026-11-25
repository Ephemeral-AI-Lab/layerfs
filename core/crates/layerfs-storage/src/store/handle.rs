//! Engine-independent two-port handle and shared bounded physical indexes.

use crate::{encoding::delta::candidates::Candidates, save::Save};
use crate::{
    error::StorageResult,
    policy::{StorageCapacities, StoragePolicy},
    port::PackPersistence,
    read::{Diagnostics, Fetch, Reader},
};
use std::{
    cell::{Cell, RefCell},
    sync::Arc,
};

/// C2's immutable-body path, independent of engines and history.
pub struct Storage {
    pub(crate) source: Fetch,
    pub(crate) work: crate::save::Work,
    policy: StoragePolicy,
    capacities: StorageCapacities,
    pub(crate) candidates: RefCell<Candidates>,
    pub(crate) reservations: crate::ReservationBlocks,
    /// The unused pack ids and ordinals of the last Save that finished on
    /// this handle, which the next Save uses before it reserves (#768).
    pub(crate) kept: Cell<Option<crate::save::IdBlock>>,
    /// Whether a finished Save keeps [`crate::policy::KEPT_ORDINALS`] unused
    /// ordinals for the next one ([`Self::keeping_ordinals`]).
    pub(crate) keeps_ordinals: bool,
}
impl Storage {
    /// Opens one handle with an acknowledged persisted policy. No service bootstrap.
    pub fn new(metadata: Arc<dyn PackPersistence>) -> StorageResult<Self> {
        Self::with_reservations(metadata, crate::ReservationBlocks::default())
    }
    /// Opens one producer handle with explicitly selected allocation windows.
    pub fn with_reservations(
        metadata: Arc<dyn PackPersistence>,
        reservations: crate::ReservationBlocks,
    ) -> StorageResult<Self> {
        let reservations = reservations.validate()?;
        let source = Fetch::new(metadata);
        source.note(|c| c.policy += 1);
        let policy = source.metadata.policy()?.validated()?;
        let capacities = StorageCapacities::from_policy(policy)?
            .with_publication_bytes(source.metadata.publication_bytes())?;
        source.capacities.set(Some(capacities));
        Ok(Self {
            source,
            work: crate::save::Work::default(),
            policy,
            capacities,
            candidates: RefCell::new(Candidates::new()?),
            reservations,
            kept: Cell::new(None),
            keeps_ordinals: false,
        })
    }
    /// A handle whose finished Saves keep up to
    /// [`crate::policy::KEPT_ORDINALS`] unused ordinals for its next Save,
    /// so a small next Save reserves nothing (#768). For a handle its owner
    /// keeps between Saves; that owner returns them with
    /// [`Self::release_kept_ordinals`] before it drops the handle in order.
    /// Any other handle's finish releases every unused ordinal, as before
    /// #768. Unused pack ids are kept either way (never reused, no release).
    #[must_use]
    pub fn keeping_ordinals(mut self) -> Self {
        self.keeps_ordinals = true;
        self
    }
    /// Returns the ordinals this handle kept from its last finished Save
    /// (#768): one publication of their release, which rewinds the Store's
    /// counter only while it still ends them (nothing was reserved after
    /// them), as a finish's release does. One attempt, never repeated: the
    /// ordinals leave the handle first, so a refused or uncertain release
    /// loses them and never reuses them. Nothing is sent when the handle
    /// keeps none (no finished Save, a Save open or dropped unfinished, or
    /// already returned). The kept pack ids stay with the handle.
    pub fn release_kept_ordinals(&self) -> StorageResult<()> {
        let Some(mut block) = self.kept.take() else {
            return Ok(());
        };
        let release = block.take_ordinals();
        self.kept.set(Some(block));
        let Some(range) = release else {
            return Ok(());
        };
        self.source.note(|c| c.publish += 1);
        self.source.metadata.publish(&crate::port::Publication {
            release_ordinals: Some(range),
            ..Default::default()
        })?;
        Ok(())
    }
    /// The end of the ordinals this handle keeps (one past the last), or
    /// `None` when it keeps none. Ordinal blocks are reserved in increasing
    /// order, so of several handles only the one whose kept ordinals end
    /// last can still end the Store's counter and be rewound by
    /// [`Self::release_kept_ordinals`].
    pub fn kept_ordinal_end(&self) -> Option<u64> {
        let block = self.kept.take();
        let end = block.as_ref().and_then(crate::save::IdBlock::ordinal_end);
        self.kept.set(block);
        end
    }
    /// Persisted, validated storage policy.
    pub const fn policy(&self) -> StoragePolicy {
        self.policy
    }
    /// Existing canonical, pack, wave and chain bounds under that policy.
    pub const fn capacities(&self) -> StorageCapacities {
        self.capacities
    }
    /// Begins one producer's bounded save. Separate handles may write concurrently.
    pub fn begin_save(&self) -> StorageResult<Save<'_>> {
        Save::new(self)
    }
    /// Creates an operation-owned authenticated reader with bounded caches.
    pub fn reader(&self) -> StorageResult<Reader<'_>> {
        Reader::new(self, false)
    }
    /// A reader that continues the latest Save's operation: besides its own
    /// bounded caches it may read the metadata pack bodies and inode values
    /// this handle's Saves published instead of fetching them back. Everything it returns is
    /// authenticated as by [`Self::reader`].
    pub fn reader_after_save(&self) -> StorageResult<Reader<'_>> {
        Reader::new(self, true)
    }
    /// Bounded recent and cumulative save-stage wall observations.
    pub fn save_work(&self) -> crate::save::SaveHistory {
        self.work.snapshot()
    }
    /// For tests and benches only: a stale delta-chain hint fails as
    /// `Integrity("delta chain hint")` instead of falling back to a
    /// level-by-level read, so a broken hint invariant fails a check (#758).
    pub fn set_strict_hints(&self, strict: bool) {
        self.source.strict_hints.set(strict);
    }
    /// Cumulative operation counts, labelled diagnostics; no timing claims.
    pub fn diagnostics(&self) -> Diagnostics {
        self.source.counters()
    }
}
