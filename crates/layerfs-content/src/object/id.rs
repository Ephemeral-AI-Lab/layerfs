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
    fn fixed_width_hex_covers_every_byte_positions_and_seeded_ids() {
        for value in u8::MIN..=u8::MAX {
            for index in [0, DIGEST_BYTES - 1] {
                let mut bytes = [0; DIGEST_BYTES];
                bytes[index] = value;
                let text = ObjectId::from_bytes(&bytes).unwrap().to_string();
                assert_eq!(text.len(), DIGEST_BYTES * 2);
                assert_eq!(&text[index * 2..index * 2 + 2], format!("{value:02x}"));
                assert_eq!(text.parse::<ObjectId>().unwrap().to_bytes(), bytes);
            }
        }
        let mut mixed = [0; DIGEST_BYTES];
        for (index, value) in [(1, 0x01), (2, 0x10), (15, 0xab), (30, 0xfe), (31, 0xff)] {
            mixed[index] = value;
        }
        let text = ObjectId::from_bytes(&mixed).unwrap().to_string();
        assert!(text.starts_with("000110"));
        assert_eq!(&text[30..32], "ab");
        assert!(text.ends_with("feff"));

        let mut state = 0x6a09_e667_f3bc_c909_u64;
        for _ in 0..128 {
            let mut bytes = [0; DIGEST_BYTES];
            for byte in &mut bytes {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                *byte = state as u8;
            }
            let id = ObjectId::from_bytes(&bytes).unwrap();
            assert_eq!(id.to_string().parse::<ObjectId>().unwrap(), id);
        }
    }

    #[test]
    fn parsing_accepts_hex_case_and_displays_canonical_lowercase() {
        let id = ObjectId::for_bytes(b"case-equivalence");
        let lowercase = id.to_string();
        let uppercase = lowercase.to_ascii_uppercase();
        assert_eq!(lowercase.parse::<ObjectId>().unwrap(), id);
        assert_eq!(uppercase.parse::<ObjectId>().unwrap(), id);
        assert_eq!(
            uppercase.parse::<ObjectId>().unwrap().to_string(),
            lowercase
        );
    }

    #[test]
    fn parsing_rejects_wrong_lengths_and_non_hex_text() {
        for value in [String::new(), "0".repeat(63), "0".repeat(65)] {
            assert_eq!(
                value.parse::<ObjectId>(),
                Err(CoreError::InvalidIdentityText)
            );
        }
        let mut non_hex = "0".repeat(DIGEST_BYTES * 2);
        non_hex.replace_range(31..32, "g");
        assert_eq!(
            non_hex.parse::<ObjectId>(),
            Err(CoreError::InvalidIdentityText)
        );
        let id = ObjectId::for_bytes(b"payload");
        assert_eq!(id.as_bytes().len(), DIGEST_BYTES);
        assert_eq!(id.to_string().parse::<ObjectId>().unwrap(), id);
        assert_eq!(ObjectId::from_reader(&b"payload"[..]).unwrap(), id);
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
    }
}
