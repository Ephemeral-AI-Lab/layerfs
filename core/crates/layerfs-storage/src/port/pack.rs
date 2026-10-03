//! Opaque immutable bodies addressed by their pack digest.

use sha2::{Digest, Sha256};

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
