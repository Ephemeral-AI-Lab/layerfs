//! Bounded metadata operations, each at most one round trip and transaction.

use crate::location::{LocatedObject, ObjectLocation, PackInfo, SignatureRow, ValueGroupRow};
use crate::policy::StoragePolicy;
use layerfs_content::ObjectId;
use std::fmt;

/// One metadata-domain pack returned with its authenticated descriptor.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MetadataPack {
    /// Persisted descriptor.
    pub info: PackInfo,
    /// Complete immutable pack bytes.
    pub body: Vec<u8>,
}

/// Bounded catalogue query: distinct ordinals or one chronological page.
#[derive(Clone, Copy, Debug)]
pub enum ValueGroupQuery<'a> {
    /// Covering rows for at most READ_OBJECT_LIMIT ordinals, absent ones omitted.
    Ordinals(&'a [u32]),
    /// At most `limit` rows starting at `from`, inclusive.
    Page {
        /// First ordinal to visit.
        from: u32,
        /// Maximum rows, at most READ_OBJECT_LIMIT.
        limit: usize,
    },
}

/// One catalogue page or set, with the retained candidate window.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValueGroups {
    /// Distinct covering rows, in increasing ordinal order.
    pub rows: Vec<ValueGroupRow>,
    /// First ordinal in the retained candidate window.
    pub window_start: u32,
    /// Start of the next page, only for a paginated query.
    pub next: Option<u32>,
}

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

/// Immutable pack registration; only metadata-domain packs carry a body.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RegisteredPack {
    /// Digest, size and routing domain.
    pub info: PackInfo,
    /// Complete metadata bytes; payload bytes were acknowledged on the other port.
    pub body: Option<Vec<u8>>,
}

/// One atomic batch, closed under direct references before this call.
#[derive(Clone, Debug, Default)]
pub struct Registration {
    /// Sealed packs, with bodies only for the metadata domain.
    pub packs: Vec<RegisteredPack>,
    /// Insert-if-absent locators; the first writer wins.
    pub objects: Vec<ObjectLocation>,
    /// Sealed pooled catalogue rows.
    pub value_groups: Vec<ValueGroupRow>,
    /// Advisory signature-ring changes published with their objects.
    pub signatures: Vec<SignatureRow>,
    /// Retained-window advance, when present.
    pub window_start: Option<u32>,
    /// Unused tail to release, as first ordinal and count.
    pub release_ordinals: Option<(u32, usize)>,
}

/// First-wins race result after successful atomic registration.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Registered {
    /// Objects whose locator was already present; C2 must compare their bytes.
    pub lost: Vec<ObjectId>,
}

/// One attempted metadata operation's outcome.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MetadataError {
    /// Required persisted state is absent.
    Missing,
    /// The provider definitely refused the operation.
    Refused {
        /// Protocol status, including a database SQLSTATE when applicable.
        status: String,
    },
    /// A response violates the port's grammar or bounds.
    Malformed,
    /// Acknowledgement or the transaction's outcome is unknown.
    Uncertain,
}
impl fmt::Display for MetadataError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "metadata store: {self:?}")
    }
}
impl std::error::Error for MetadataError {}

/// Committed physical metadata. Each call is one attempt, with no recovery loop.
pub trait MetadataStore: Send + Sync {
    /// Reads the persisted profile once per C2 handle.
    fn policy(&self) -> Result<StoragePolicy, MetadataError>;
    /// Locates at most READ_OBJECT_LIMIT distinct ids; absence is omitted.
    fn locate(&self, ids: &[ObjectId], out: &mut Vec<LocatedObject>) -> Result<(), MetadataError>;
    /// Reads complete metadata packs totalling at most DEPENDENCY_PACK_CACHE_BYTES.
    fn read_packs(&self, ids: &[i64], out: &mut Vec<MetadataPack>) -> Result<(), MetadataError>;
    /// Reads one bounded catalogue set or page.
    fn value_groups(&self, query: ValueGroupQuery<'_>) -> Result<ValueGroups, MetadataError>;
    /// Reads at most 8,192 signature rows, in insertion order, once per handle.
    fn signatures(&self, out: &mut Vec<SignatureRow>) -> Result<(), MetadataError>;
    /// Allocates one block of pack ids and/or pooled ordinals.
    fn reserve(&self, request: Reserve) -> Result<Reserved, MetadataError>;
    /// Atomically registers at most TRANSACTION_ROW_LIMIT rows and the canonical
    /// byte budget, with first-wins object insertion. All metadata bodies, pooled
    /// rows, signature changes and ordinal/window changes belong to this unit.
    fn register(&self, batch: &Registration) -> Result<Registered, MetadataError>;
}
