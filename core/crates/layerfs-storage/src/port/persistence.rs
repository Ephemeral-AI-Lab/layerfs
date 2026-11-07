//! Bounded atomic physical persistence; one transaction attempt per operation.

use super::{ObjectKey, PackReadPlan, PersistedPack, PersistedPackRead};
use crate::location::{
    InodeValueRow, LocatedObject, ObjectLocation, PackDomain, PackInfo, SignatureRow,
};
use crate::policy::StoragePolicy;
use layerfs_content::ObjectId;
use std::{fmt, sync::Arc};

/// Requested allocation block. A zero count requests no allocation of that kind.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Reserve {
    /// Number of consecutive pack ids.
    pub packs: usize,
    /// Number of consecutive pooled ordinals.
    pub ordinals: usize,
}

/// Allocated blocks; never infer a reservation from an unacknowledged response.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Reserved {
    /// First pack id, positive when packs were requested.
    pub first_pack_id: i64,
    /// First ordinal, positive when ordinals were requested.
    pub first_ordinal: u32,
}

/// Immutable pack registration; all domains carry a body.
///
/// The digest is computed once, when Storage seals the pack: [`Self::seal`]
/// is the only constructor and hashes the body it is given. Private fields
/// keep the descriptor bound to those exact bytes, so Persistence trusts
/// `info().key` and does not hash the body again.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PublishedPack {
    info: PackInfo,
    body: Arc<Vec<u8>>,
}
impl PublishedPack {
    /// Seals `body` as pack `pack_id` in `domain`: its length and SHA-256.
    pub fn seal(pack_id: i64, domain: PackDomain, body: Arc<Vec<u8>>) -> Self {
        let info = PackInfo {
            pack_id,
            domain,
            key: ObjectKey::for_bytes(&body),
            length: body.len(),
        };
        Self { info, body }
    }
    /// Digest, size and routing domain of [`Self::body`].
    pub const fn info(&self) -> PackInfo {
        self.info
    }
    /// Complete immutable bytes, acknowledged atomically with locators.
    pub fn body(&self) -> &Arc<Vec<u8>> {
        &self.body
    }
}

/// One atomic batch, closed under direct references before this call.
#[derive(Clone, Debug, Default)]
pub struct Publication {
    /// Sealed packs, with complete bodies for both domains.
    pub packs: Vec<PublishedPack>,
    /// Insert-if-absent locators; the first writer wins.
    pub objects: Vec<ObjectLocation>,
    /// New inode values, each under an ordinal of this Save's reserved block.
    pub inode_values: Vec<InodeValueRow>,
    /// Advisory signature-ring changes published with their objects.
    pub signatures: Vec<SignatureRow>,
    /// Unused tail to release, as first ordinal and count.
    pub release_ordinals: Option<(u32, usize)>,
}

/// First-wins race result after successful atomic registration.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Published {
    /// Objects whose locator was already present; C2 must compare their bytes.
    pub lost: Vec<ObjectId>,
}

/// One attempted metadata operation's outcome.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PersistenceError {
    /// The write lock or session is held; no write effect occurred.
    Busy,
    /// Selected engine is unavailable; no fallback is attempted.
    BackendUnavailable,
    /// Required persisted state is absent.
    Missing,
    /// The provider definitely refused the operation.
    Refused {
        /// Explicit backend refusal class.
        status: String,
    },
    /// A response violates the port's grammar or bounds.
    Malformed,
    /// Acknowledgement or the transaction's outcome is unknown.
    Uncertain,
}
impl fmt::Display for PersistenceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "pack persistence: {self:?}")
    }
}
impl std::error::Error for PersistenceError {}

/// Committed immutable physical storage. Each call is one attempt, with no recovery loop.
pub trait PackPersistence: Send + Sync {
    /// Reads the persisted profile once per C2 handle.
    fn policy(&self) -> Result<StoragePolicy, PersistenceError>;
    /// Locates at most READ_OBJECT_LIMIT distinct ids; absence is omitted.
    fn locate(
        &self,
        ids: &[ObjectId],
        out: &mut Vec<LocatedObject>,
    ) -> Result<(), PersistenceError>;
    /// Reads complete packs totalling at most DEPENDENCY_PACK_CACHE_BYTES,
    /// or one separately bounded singleton up to SINGLETON_PACK_LIMIT.
    /// Results use `PersistedPack::authenticate`: length/digest verified once,
    /// immutable across this boundary. C2 retains framing/binding checks.
    fn read_packs(&self, ids: &[i64], out: &mut Vec<PersistedPack>)
        -> Result<(), PersistenceError>;
    /// Acquires one immutable pack with storage-owned range planning. Complete
    /// SHA256 authentication is mandatory even when only selected bytes return.
    /// Unsupported providers fail explicitly, with no error-driven whole fallback.
    fn read_pack_selection(
        &self,
        _id: i64,
        _plan: &mut dyn PackReadPlan,
    ) -> Result<PersistedPackRead, PersistenceError> {
        Err(PersistenceError::BackendUnavailable)
    }
    /// Localized physical acquisition for canonical consumers. Returned units do
    /// not assert whole-pack integrity; every used canonical object/dependency
    /// or catalogue value must be authenticated by storage before use/return.
    /// Complete pack audits continue through read_packs/read_pack_selection.
    fn read_scoped_pack(
        &self,
        _id: i64,
        _plan: &mut dyn PackReadPlan,
    ) -> Result<super::AcquiredPackRead, PersistenceError> {
        Err(PersistenceError::BackendUnavailable)
    }
    /// Reads the stored inode values of at most READ_OBJECT_LIMIT distinct
    /// ascending ordinals in one statement execution of one transaction. Rows
    /// come back in ordinal order; an ordinal with no stored value is omitted.
    fn inode_values(
        &self,
        ordinals: &[u32],
        out: &mut Vec<InodeValueRow>,
    ) -> Result<(), PersistenceError>;
    /// Reads at most 8,192 signature rows, in insertion order, once per handle.
    fn signatures(&self, out: &mut Vec<SignatureRow>) -> Result<(), PersistenceError>;
    /// Allocates one block of pack ids and/or pooled ordinals.
    fn reserve(&self, request: Reserve) -> Result<Reserved, PersistenceError>;
    /// Largest pack a localized acquisition takes whole and keeps for later
    /// demands. A provider whose every ranged read is a network round trip
    /// raises it to the pack limit; the default takes only packs no larger
    /// than their control prefix. Bounded caches still bound what is kept.
    fn whole_pack_bytes(&self) -> usize {
        super::PACK_READ_PREFIX_BYTES
    }
    /// Canonical bytes one preparation wave holds and one publication carries,
    /// and the sealed bytes it publishes. The default is the shared
    /// transaction bound, 4 MiB less one byte; a provider whose every
    /// statement is a network round trip may take more, within
    /// [`crate::policy::PUBLICATION_BYTES_MAX`], so fewer publications carry
    /// the same Save. The Save holds up to this many bytes of pending objects
    /// and sealed packs before it publishes them.
    fn publication_bytes(&self) -> u64 {
        crate::policy::TRANSACTION_CANONICAL_BYTES_LIMIT
    }
    /// Prospective physical rows and submitted bytes for one immutable pack.
    /// Monolithic providers use one row and the original body-byte charge.
    /// Providers with unit fanout return its exact cost before publication.
    fn publication_pack_cost(
        &self,
        pack: &PublishedPack,
    ) -> Result<(usize, u64), PersistenceError> {
        Ok((1, pack.body().len() as u64))
    }
    /// Atomically registers at most TRANSACTION_ROW_LIMIT rows and the canonical
    /// byte budget, with first-wins object insertion. All sealed bodies, inode
    /// value rows, signature changes and ordinal changes belong to this unit.
    fn publish(&self, batch: &Publication) -> Result<Published, PersistenceError>;
}
