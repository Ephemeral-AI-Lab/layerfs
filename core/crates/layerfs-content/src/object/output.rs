//! Finalized canonical output: the owned object handed to a bounded consumer.
//!
//! Construction emits an object only after its bytes, role and direct references
//! are established, and only when later input cannot change them. Ownership of
//! the allocation moves to the consumer; C1 keeps no payload copy.

use crate::error::{ContentError, ContentResult};
use crate::object::{codec, ObjectId};

/// Logical meaning of a canonical object; not a physical FULL/DELTA choice.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum ObjectRole {
    /// Complete payload of a nonempty regular file below the construction cutoff.
    WholeFile,
    /// One canonical chunk payload produced by the frozen CDC profile.
    Chunk,
    /// Extent-tree leaf: a page of payload slices.
    ExtentLeaf,
    /// Extent-tree branch: a page of child node summaries.
    ExtentBranch,
    /// File state: the logical root of a chunked file.
    FileState,
}

impl ObjectRole {
    /// Stable persisted role code.
    pub const fn code(self) -> u8 {
        match self {
            Self::WholeFile => 1,
            Self::Chunk => 2,
            Self::ExtentLeaf => 3,
            Self::ExtentBranch => 4,
            Self::FileState => 5,
        }
    }

    /// Rebuilds a role from its persisted code.
    pub const fn from_code(code: u8) -> ContentResult<Self> {
        match code {
            1 => Ok(Self::WholeFile),
            2 => Ok(Self::Chunk),
            3 => Ok(Self::ExtentLeaf),
            4 => Ok(Self::ExtentBranch),
            5 => Ok(Self::FileState),
            _ => Err(ContentError::InvalidRecord("object role code")),
        }
    }
}

/// One finalized canonical object: identity, role, owned bytes, direct references.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FinalizedObject {
    id: ObjectId,
    role: ObjectRole,
    canonical: Vec<u8>,
    references: Vec<ObjectId>,
}

impl FinalizedObject {
    /// Wraps already-canonical bytes and computes their identity once.
    pub fn new(role: ObjectRole, canonical: Vec<u8>) -> ContentResult<Self> {
        // A leaf or whole-file object must still be a decodable canonical object.
        codec::decode_bytes_object(&canonical)?;
        let id = ObjectId::for_bytes(&canonical);
        Ok(Self {
            id,
            role,
            canonical,
            references: Vec::new(),
        })
    }

    /// Attaches the direct logical references this object was built from.
    pub fn with_references(mut self, references: Vec<ObjectId>) -> Self {
        self.references = references;
        self
    }

    /// Identity of the canonical bytes.
    pub const fn id(&self) -> ObjectId {
        self.id
    }

    /// Established semantic role.
    pub const fn role(&self) -> ObjectRole {
        self.role
    }

    /// Canonical bytes, including the envelope.
    pub fn canonical(&self) -> &[u8] {
        &self.canonical
    }

    /// Canonical length in bytes.
    pub fn canonical_len(&self) -> usize {
        self.canonical.len()
    }

    /// Capacity charged to the owner, not just the used length.
    pub fn canonical_capacity(&self) -> usize {
        self.canonical.capacity()
    }

    /// Direct logical child identities, in canonical order.
    pub fn references(&self) -> &[ObjectId] {
        &self.references
    }

    /// Moves the owned pieces to a consumer that stores or forwards them.
    pub fn into_parts(self) -> (ObjectId, ObjectRole, Vec<u8>, Vec<ObjectId>) {
        (self.id, self.role, self.canonical, self.references)
    }
}

/// Bounded sink for finalized canonical objects.
///
/// `accept` takes ownership. Returning an error ends construction: the caller
/// receives that error once and no retry, resend or alternative path is taken.
pub trait FinalizedConsumer {
    /// Accepts one finalized object.
    fn accept(&mut self, object: FinalizedObject) -> ContentResult<()>;
}

/// Non-persisting consumer that charges and counts what construction emitted.
///
/// It performs no storage work, so construction can complete with no database,
/// pack or file involved. Its counters describe the emitted object set, not a
/// stored one.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DiscardingConsumer {
    objects: u64,
    canonical_bytes: u64,
    peak_object_bytes: u64,
}

impl DiscardingConsumer {
    /// Empty consumer.
    pub const fn new() -> Self {
        Self {
            objects: 0,
            canonical_bytes: 0,
            peak_object_bytes: 0,
        }
    }

    /// Number of accepted objects.
    pub const fn objects(self) -> u64 {
        self.objects
    }

    /// Total accepted canonical bytes.
    pub const fn canonical_bytes(self) -> u64 {
        self.canonical_bytes
    }

    /// Largest single accepted canonical object.
    pub const fn peak_object_bytes(self) -> u64 {
        self.peak_object_bytes
    }
}

impl FinalizedConsumer for DiscardingConsumer {
    fn accept(&mut self, object: FinalizedObject) -> ContentResult<()> {
        let length = object.canonical_len() as u64;
        self.objects = self.objects.saturating_add(1);
        self.canonical_bytes = self.canonical_bytes.saturating_add(length);
        self.peak_object_bytes = self.peak_object_bytes.max(length);
        Ok(())
    }
}
