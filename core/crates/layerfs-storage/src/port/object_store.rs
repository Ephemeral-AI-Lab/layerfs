//! Opaque immutable bodies addressed by their pack digest.

use sha2::{Digest, Sha256};
use std::fmt;

/// SHA-256 identity of a sealed pack, computed and checked by C2.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct ObjectKey([u8; 32]);

impl ObjectKey {
    /// Hashes the complete sealed bytes.
    pub fn for_bytes(bytes: &[u8]) -> Self {
        Self(Sha256::digest(bytes).into())
    }
    /// Imports a persisted digest.
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }
    /// Digest bytes for an engine's opaque key.
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

/// Half-open bounded byte range; the engine refuses an invalid range.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ByteRange {
    /// Inclusive first byte.
    pub start: u64,
    /// Number of bytes demanded, strictly positive.
    pub length: u64,
}

/// Conditional creation result; an existing immutable key is never overwritten.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Put {
    /// The body was created.
    Created,
    /// The key was already present.
    AlreadyPresent,
}

/// One attempted object-store operation's outcome.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ObjectError {
    /// The requested key does not exist.
    Missing,
    /// The provider definitely refused the operation.
    Refused {
        /// Protocol status returned by the provider.
        status: u16,
    },
    /// A response violates the port's grammar or bounds.
    Malformed,
    /// Acknowledgement or the persistence outcome is unknown.
    Uncertain,
}

impl fmt::Display for ObjectError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "object store: {self:?}")
    }
}
impl std::error::Error for ObjectError {}

/// One-attempt I/O for opaque sealed bodies; no retry, reconnect or fallback.
pub trait ObjectStore: Send + Sync {
    /// Creates only when absent. Never alters an existing key.
    fn put_if_absent(&self, key: ObjectKey, body: &[u8]) -> Result<Put, ObjectError>;
    /// Replaces `out` with the whole body or exact requested range.
    fn read(
        &self,
        key: ObjectKey,
        range: Option<ByteRange>,
        out: &mut Vec<u8>,
    ) -> Result<(), ObjectError>;
    /// Returns the stored body length, or absence.
    fn head(&self, key: ObjectKey) -> Result<Option<u64>, ObjectError>;
}
