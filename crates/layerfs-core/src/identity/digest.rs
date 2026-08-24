use std::io::{self, Read, Write};

use crate::error::{CoreError, CoreResult};

pub const DIGEST_BYTES: usize = 32;
const OBJECT_DOMAIN: &[u8] = b"layerfs/object\0";

pub struct ContentDigestWriter {
    hasher: blake3::Hasher,
}

impl ContentDigestWriter {
    pub fn new() -> Self {
        Self {
            hasher: blake3::Hasher::new(),
        }
    }

    pub fn finish(self) -> [u8; DIGEST_BYTES] {
        *self.hasher.finalize().as_bytes()
    }
}

impl Default for ContentDigestWriter {
    fn default() -> Self {
        Self::new()
    }
}

impl Write for ContentDigestWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.hasher.update(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

pub(crate) struct ObjectHashWriter {
    hasher: blake3::Hasher,
}

impl ObjectHashWriter {
    pub(crate) fn new() -> Self {
        let mut hasher = blake3::Hasher::new();
        hasher.update(OBJECT_DOMAIN);
        Self { hasher }
    }

    pub(crate) fn finish(self) -> [u8; DIGEST_BYTES] {
        *self.hasher.finalize().as_bytes()
    }

    pub(crate) fn update(&mut self, bytes: &[u8]) {
        self.hasher.update(bytes);
    }
}

impl Write for ObjectHashWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.hasher.update(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn digest_object(bytes: &[u8]) -> [u8; DIGEST_BYTES] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(OBJECT_DOMAIN);
    hasher.update(bytes);
    *hasher.finalize().as_bytes()
}

fn digest_object_reader<R: Read>(reader: &mut R) -> io::Result<[u8; DIGEST_BYTES]> {
    let mut hasher = blake3::Hasher::new();
    hasher.update(OBJECT_DOMAIN);
    let mut buffer = [0_u8; 8192];
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(*hasher.finalize().as_bytes())
}

pub fn hash_object_bytes(bytes: &[u8]) -> [u8; DIGEST_BYTES] {
    digest_object(bytes)
}

pub(crate) fn hash_object_reader<R: Read>(mut reader: R) -> io::Result<[u8; DIGEST_BYTES]> {
    digest_object_reader(&mut reader)
}

pub(crate) fn digest_from_bytes(bytes: &[u8]) -> CoreResult<[u8; DIGEST_BYTES]> {
    bytes
        .try_into()
        .map_err(|_| CoreError::InvalidIdentityLength {
            expected: DIGEST_BYTES,
            actual: bytes.len(),
        })
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;

    #[test]
    fn object_hash_is_fragmentation_independent() {
        let contiguous = hash_object_bytes(b"fragmented input");
        let mut reader = Cursor::new(b"fragmented input");
        assert_eq!(contiguous, hash_object_reader(&mut reader).unwrap());
    }

    #[test]
    fn object_domain_is_not_unframed_hashing() {
        assert_ne!(
            hash_object_bytes(b"same"),
            *blake3::hash(b"same").as_bytes()
        );
    }

    #[test]
    fn content_digest_writer_is_fragmentation_independent() {
        let mut fragmented = ContentDigestWriter::new();
        fragmented.write_all(b"fragmented ").unwrap();
        fragmented.write_all(b"input").unwrap();

        assert_eq!(
            fragmented.finish(),
            *blake3::hash(b"fragmented input").as_bytes()
        );
    }
}
