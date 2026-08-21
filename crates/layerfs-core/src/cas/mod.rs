//! Immutable content-addressed chunk storage semantics.

use std::collections::BTreeMap;

use crate::cdc::MAXIMUM_CHUNK_BYTES;
use crate::identity::ObjectId;
use crate::object::{decode_bytes_object, encode_bytes_object, validate_bytes_identity};
use crate::{CoreError, CoreResult};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PutOutcome {
    Inserted,
    Reused,
}

#[derive(Debug, Default)]
pub struct InMemoryCas {
    objects: BTreeMap<ObjectId, Vec<u8>>,
    stored_bytes: u64,
}

impl InMemoryCas {
    pub const fn new() -> Self {
        Self {
            objects: BTreeMap::new(),
            stored_bytes: 0,
        }
    }

    pub fn put(&mut self, id: ObjectId, bytes: &[u8]) -> CoreResult<PutOutcome> {
        if bytes.len() > MAXIMUM_CHUNK_BYTES {
            return Err(CoreError::ObjectLimitExceeded);
        }
        let canonical = encode_bytes_object(bytes)?;
        if ObjectId::for_bytes(&canonical) != id {
            return Err(CoreError::IdentityMismatch);
        }

        self.put_verified(id, canonical)
    }

    fn put_verified(&mut self, id: ObjectId, canonical: Vec<u8>) -> CoreResult<PutOutcome> {
        if let Some(existing) = self.objects.get(&id) {
            validate_bytes_identity(existing, id)?;
            if existing != &canonical {
                return Err(CoreError::IdentityMismatch);
            }
            return Ok(PutOutcome::Reused);
        }

        let byte_len = u64::try_from(canonical.len()).map_err(|_| CoreError::LengthOverflow)?;
        let stored_bytes = self
            .stored_bytes
            .checked_add(byte_len)
            .ok_or(CoreError::LengthOverflow)?;
        self.objects.insert(id, canonical);
        self.stored_bytes = stored_bytes;
        Ok(PutOutcome::Inserted)
    }

    pub fn put_chunk(&mut self, bytes: &[u8]) -> CoreResult<(ObjectId, PutOutcome)> {
        if bytes.len() > MAXIMUM_CHUNK_BYTES {
            return Err(CoreError::ObjectLimitExceeded);
        }
        let canonical = encode_bytes_object(bytes)?;
        let id = ObjectId::for_bytes(&canonical);
        let outcome = self.put_verified(id, canonical)?;
        Ok((id, outcome))
    }

    pub fn get(&self, id: ObjectId) -> CoreResult<&[u8]> {
        let canonical = self.objects.get(&id).ok_or(CoreError::MissingObject)?;
        validate_bytes_identity(canonical, id)?;
        decode_bytes_object(canonical)
    }

    pub fn object_count(&self) -> usize {
        self.objects.len()
    }

    pub const fn stored_bytes(&self) -> u64 {
        self.stored_bytes
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;
    use crate::cdc::FastCdc;
    use crate::{encode_bytes_object, ObjectId};

    fn input(len: usize) -> Vec<u8> {
        let mut state = 0x9e37_79b9_7f4a_7c15u64;
        (0..len)
            .map(|_| {
                state ^= state.wrapping_shl(7);
                state ^= state.wrapping_shr(9);
                state ^= state.wrapping_shl(8);
                state as u8
            })
            .collect()
    }

    #[test]
    fn chunk_identity_authenticates_complete_canonical_bytes() {
        let bytes = b"chunk bytes";
        let canonical = encode_bytes_object(bytes).unwrap();
        let mut cas = InMemoryCas::new();
        let (id, _) = cas.put_chunk(bytes).unwrap();
        assert_eq!(id, ObjectId::for_bytes(&canonical));
        assert_ne!(id, ObjectId::for_bytes(bytes));
        assert_eq!(cas.objects.get(&id), Some(&canonical));
    }

    #[test]
    fn scan_identity_deduplicates_and_preserves_callback_bytes() {
        let data = input(100_000);
        let mut cas = InMemoryCas::new();
        let mut observed = Vec::new();
        let mut ids = Vec::new();

        FastCdc::new()
            .scan(Cursor::new(data.clone()), |chunk| {
                observed.push(chunk.to_vec());
                let (id, outcome) = cas.put_chunk(chunk)?;
                assert!(matches!(outcome, PutOutcome::Inserted | PutOutcome::Reused));
                ids.push(id);
                Ok(())
            })
            .unwrap();

        assert_eq!(observed.concat(), data);
        for (id, expected) in ids.iter().zip(&observed) {
            assert_eq!(cas.get(*id).unwrap(), expected.as_slice());
        }

        let stored_bytes = cas.stored_bytes();
        let mut repeated_ids = Vec::new();
        FastCdc::new()
            .scan(Cursor::new(data), |chunk| {
                let (id, outcome) = cas.put_chunk(chunk)?;
                assert_eq!(outcome, PutOutcome::Reused);
                repeated_ids.push(id);
                Ok(())
            })
            .unwrap();
        assert_eq!(repeated_ids, ids);
        assert_eq!(cas.stored_bytes(), stored_bytes);

        let equal = b"equal chunk";
        let id = ObjectId::for_bytes(&encode_bytes_object(equal).unwrap());
        let object_count = cas.object_count();
        assert_eq!(cas.put(id, equal), Ok(PutOutcome::Inserted));
        assert_eq!(cas.put(id, equal), Ok(PutOutcome::Reused));
        assert_eq!(cas.object_count(), object_count + 1);
    }

    #[test]
    fn rejects_missing_malformed_and_unequal_replacement() {
        let bytes = b"immutable";
        let id = ObjectId::for_bytes(&encode_bytes_object(bytes).unwrap());
        let mut cas = InMemoryCas::new();

        assert_eq!(cas.get(id), Err(CoreError::MissingObject));
        assert_eq!(
            cas.put(id, b"wrong bytes"),
            Err(CoreError::IdentityMismatch)
        );
        assert_eq!(cas.put(id, bytes), Ok(PutOutcome::Inserted));
        assert_eq!(
            cas.put(id, b"wrong bytes"),
            Err(CoreError::IdentityMismatch)
        );
        assert_eq!(cas.get(id), Ok(bytes.as_slice()));

        cas.objects.insert(id, b"corrupted incumbent".to_vec());
        assert_eq!(cas.get(id), Err(CoreError::IdentityMismatch));
        assert_eq!(cas.put(id, bytes), Err(CoreError::IdentityMismatch));
        assert_eq!(cas.objects.get(&id), Some(&b"corrupted incumbent".to_vec()));
    }

    #[test]
    fn authenticated_range_returns_nothing_for_a_well_formed_identity_substitution() {
        let bytes = b"authenticated";
        let mut cas = InMemoryCas::new();
        let (id, _) = cas.put_chunk(bytes).unwrap();
        let file = crate::LogicalFile::from_chunks(
            &cas,
            vec![crate::ChunkReference::new(id, bytes.len() as u64)],
        )
        .unwrap();
        cas.objects
            .insert(id, encode_bytes_object(b"same-shape-evil").unwrap());
        assert_eq!(cas.get(id), Err(CoreError::IdentityMismatch));
        assert_eq!(
            file.read_range(&cas, 0..bytes.len() as u64),
            Err(CoreError::IdentityMismatch)
        );
    }

    #[test]
    fn chunk_storage_is_bounded_by_the_cdc_maximum() {
        let mut cas = InMemoryCas::new();
        let oversized = vec![0_u8; MAXIMUM_CHUNK_BYTES + 1];
        assert_eq!(
            cas.put_chunk(&oversized),
            Err(CoreError::ObjectLimitExceeded)
        );

        let data = input(MAXIMUM_CHUNK_BYTES * 2 + 1);
        FastCdc::new()
            .scan(Cursor::new(data), |chunk| {
                assert!(chunk.len() <= MAXIMUM_CHUNK_BYTES);
                cas.put_chunk(chunk)?;
                Ok(())
            })
            .unwrap();
        let exact = cas.objects.values().try_fold(0_u64, |total, canonical| {
            total.checked_add(u64::try_from(canonical.len()).unwrap())
        });
        assert_eq!(cas.stored_bytes(), exact.unwrap());
        assert!(cas.objects.values().all(|canonical| {
            decode_bytes_object(canonical).is_ok_and(|raw| canonical.len() == raw.len() + 13)
        }));
    }
}
