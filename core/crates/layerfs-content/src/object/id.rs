//! Canonical object identity: a fixed 32-byte digest over frozen framing.
//!
//! Identity is a BLAKE3 digest of a frozen domain separator followed by the
//! canonical object bytes, so it is not a raw content digest of the payload and
//! identical canonical bytes always produce the same ID.

use std::fmt::{self, Write};
use std::str::FromStr;

use crate::error::{ContentError, ContentResult};

/// Width of a canonical object identity in bytes.
pub const DIGEST_BYTES: usize = 32;

/// Frozen domain separator for v2 object identity.
pub const OBJECT_DOMAIN: &[u8] = b"layerfs/object/v2\0";

/// Immutable canonical object identity.
#[derive(Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct ObjectId([u8; DIGEST_BYTES]);

impl ObjectId {
    /// Identity of `canonical` under the frozen object domain.
    pub fn for_bytes(canonical: &[u8]) -> Self {
        let mut hasher = blake3::Hasher::new();
        hasher.update(OBJECT_DOMAIN);
        hasher.update(canonical);
        Self(*hasher.finalize().as_bytes())
    }

    /// Streams `canonical` through the same domain-separated hash.
    pub fn for_reader<R: std::io::Read>(mut reader: R) -> std::io::Result<Self> {
        let mut hasher = blake3::Hasher::new();
        hasher.update(OBJECT_DOMAIN);
        let mut buffer = [0_u8; 8_192];
        loop {
            let read = reader.read(&mut buffer)?;
            if read == 0 {
                return Ok(Self(*hasher.finalize().as_bytes()));
            }
            hasher.update(&buffer[..read]);
        }
    }

    /// Rebuilds an identity from its fixed-width representation.
    pub fn from_bytes(bytes: &[u8]) -> ContentResult<Self> {
        let digest: [u8; DIGEST_BYTES] =
            bytes
                .try_into()
                .map_err(|_| ContentError::InvalidIdentityLength {
                    expected: DIGEST_BYTES,
                    actual: bytes.len(),
                })?;
        Ok(Self(digest))
    }

    /// Borrows the fixed-width representation.
    pub const fn as_bytes(&self) -> &[u8; DIGEST_BYTES] {
        &self.0
    }

    /// Copies the fixed-width representation.
    pub const fn to_bytes(self) -> [u8; DIGEST_BYTES] {
        self.0
    }
}

impl fmt::Debug for ObjectId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("ObjectId")
            .field(&self.to_string())
            .finish()
    }
}

impl fmt::Display for ObjectId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        for byte in self.0 {
            formatter.write_char(char::from(HEX[usize::from(byte >> 4)]))?;
            formatter.write_char(char::from(HEX[usize::from(byte & 0x0f)]))?;
        }
        Ok(())
    }
}

impl FromStr for ObjectId {
    type Err = ContentError;

    fn from_str(value: &str) -> ContentResult<Self> {
        if value.len() != DIGEST_BYTES * 2 {
            return Err(ContentError::InvalidIdentityText);
        }
        let mut bytes = [0_u8; DIGEST_BYTES];
        for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
            bytes[index] = (hex(pair[0])? << 4) | hex(pair[1])?;
        }
        Ok(Self(bytes))
    }
}

fn hex(value: u8) -> ContentResult<u8> {
    match value {
        b'0'..=b'9' => Ok(value - b'0'),
        b'a'..=b'f' => Ok(value - b'a' + 10),
        b'A'..=b'F' => Ok(value - b'A' + 10),
        _ => Err(ContentError::InvalidIdentityText),
    }
}

/// Authenticates `canonical` against a caller-supplied expected identity.
pub fn authenticate(expected: ObjectId, canonical: &[u8]) -> ContentResult<()> {
    if ObjectId::for_bytes(canonical) == expected {
        Ok(())
    } else {
        Err(ContentError::IdentityMismatch)
    }
}
