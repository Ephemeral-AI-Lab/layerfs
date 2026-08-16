use std::fmt::{self, Write};
use std::str::FromStr;

use super::digest::{digest_from_bytes, hash_object_bytes, hash_object_reader};
use super::DIGEST_BYTES;
use crate::error::{CoreError, CoreResult};

#[derive(Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct ObjectId([u8; DIGEST_BYTES]);

impl ObjectId {
    pub(crate) const fn from_digest(digest: [u8; DIGEST_BYTES]) -> Self {
        Self(digest)
    }

    pub fn for_bytes(bytes: &[u8]) -> Self {
        Self(hash_object_bytes(bytes))
    }

    pub fn from_bytes(bytes: &[u8]) -> CoreResult<Self> {
        Ok(Self(digest_from_bytes(bytes)?))
    }

    pub fn as_bytes(&self) -> &[u8; DIGEST_BYTES] {
        &self.0
    }

    pub fn to_bytes(self) -> [u8; DIGEST_BYTES] {
        self.0
    }

    pub fn from_reader<R: std::io::Read>(reader: R) -> std::io::Result<Self> {
        Ok(Self(hash_object_reader(reader)?))
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
    type Err = CoreError;

    fn from_str(value: &str) -> CoreResult<Self> {
        if value.len() != DIGEST_BYTES * 2 {
            return Err(CoreError::InvalidIdentityText);
        }
        let mut bytes = [0_u8; DIGEST_BYTES];
        for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
            bytes[index] = (hex(pair[0])? << 4) | hex(pair[1])?;
        }
        Ok(Self(bytes))
    }
}

fn hex(value: u8) -> CoreResult<u8> {
    match value {
        b'0'..=b'9' => Ok(value - b'0'),
        b'a'..=b'f' => Ok(value - b'a' + 10),
        b'A'..=b'F' => Ok(value - b'A' + 10),
        _ => Err(CoreError::InvalidIdentityText),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_fixed_and_text_round_trips() {
        let id = ObjectId::for_bytes(b"payload");
        assert_eq!(id.as_bytes().len(), DIGEST_BYTES);
        assert_eq!(id.to_string().parse::<ObjectId>().unwrap(), id);
        assert_eq!(
            ObjectId::from_bytes(&[0; DIGEST_BYTES]).unwrap().to_bytes(),
            [0; DIGEST_BYTES]
        );
        assert_eq!(
            ObjectId::from_bytes(&[0; DIGEST_BYTES - 1]),
            Err(CoreError::InvalidIdentityLength {
                expected: DIGEST_BYTES,
                actual: DIGEST_BYTES - 1
            })
        );
        assert_eq!(
            "zz".parse::<ObjectId>(),
            Err(CoreError::InvalidIdentityText)
        );
    }
}
