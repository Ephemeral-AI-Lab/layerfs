//! Immutable content-addressed chunk storage semantics.

use std::collections::BTreeMap;

use crate::cdc::MAXIMUM_CHUNK_BYTES;
use crate::identity::{chunk_id, ChunkId};
use crate::{CoreError, CoreResult};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PutOutcome {
    Inserted,
    Reused,
}

#[derive(Debug, Default)]
pub struct InMemoryCas {
    objects: BTreeMap<ChunkId, Vec<u8>>,
    stored_bytes: u64,
}

impl InMemoryCas {
    pub const fn new() -> Self {
        Self {
            objects: BTreeMap::new(),
            stored_bytes: 0,
        }
    }

    pub fn put(&mut self, id: ChunkId, bytes: &[u8]) -> CoreResult<PutOutcome> {
        if bytes.len() > MAXIMUM_CHUNK_BYTES {
            return Err(CoreError::ObjectLimitExceeded);
        }
        if chunk_id(bytes) != id {
            return Err(CoreError::IdentityMismatch);
        }

        self.put_verified(id, bytes)
    }

    fn put_verified(&mut self, id: ChunkId, bytes: &[u8]) -> CoreResult<PutOutcome> {
        if bytes.len() > MAXIMUM_CHUNK_BYTES {
            return Err(CoreError::ObjectLimitExceeded);
        }

        if let Some(existing) = self.objects.get(&id) {
            if chunk_id(existing) != id || existing != bytes {
                return Err(CoreError::IdentityMismatch);
            }
            return Ok(PutOutcome::Reused);
        }

        let byte_len = u64::try_from(bytes.len()).map_err(|_| CoreError::LengthOverflow)?;
        let stored_bytes = self
            .stored_bytes
            .checked_add(byte_len)
            .ok_or(CoreError::LengthOverflow)?;
        self.objects.insert(id, bytes.to_vec());
        self.stored_bytes = stored_bytes;
        Ok(PutOutcome::Inserted)
    }

    pub fn put_chunk(&mut self, bytes: &[u8]) -> CoreResult<(ChunkId, PutOutcome)> {
        if bytes.len() > MAXIMUM_CHUNK_BYTES {
            return Err(CoreError::ObjectLimitExceeded);
        }
        let id = chunk_id(bytes);
        let outcome = self.put_verified(id, bytes)?;
        Ok((id, outcome))
    }

    pub fn get(&self, id: ChunkId) -> CoreResult<&[u8]> {
        let bytes = self.objects.get(&id).ok_or(CoreError::MissingObject)?;
        if chunk_id(bytes) != id {
            return Err(CoreError::IdentityMismatch);
        }
        Ok(bytes)
    }

    pub fn object_count(&self) -> usize {
        self.objects.len()
    }

    pub const fn stored_bytes(&self) -> u64 {
        self.stored_bytes
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ChunkLocation {
    offset: u64,
    length: u32,
}

#[derive(Debug, Default)]
pub struct PackedInMemoryCas {
    payload: Vec<u8>,
    index: BTreeMap<ChunkId, ChunkLocation>,
    stored_bytes: u64,
    payload_reallocations: u64,
    payload_growth_copy_estimate: u64,
}

impl PackedInMemoryCas {
    pub fn new() -> Self {
        Self {
            payload: Vec::new(),
            index: BTreeMap::new(),
            stored_bytes: 0,
            payload_reallocations: 0,
            payload_growth_copy_estimate: 0,
        }
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            payload: Vec::with_capacity(capacity),
            ..Self::new()
        }
    }

    pub fn put(&mut self, id: ChunkId, bytes: &[u8]) -> CoreResult<PutOutcome> {
        if bytes.len() > MAXIMUM_CHUNK_BYTES {
            return Err(CoreError::ObjectLimitExceeded);
        }
        if chunk_id(bytes) != id {
            return Err(CoreError::IdentityMismatch);
        }

        self.put_verified(id, bytes)
    }

    fn put_verified(&mut self, id: ChunkId, bytes: &[u8]) -> CoreResult<PutOutcome> {
        if bytes.len() > MAXIMUM_CHUNK_BYTES {
            return Err(CoreError::ObjectLimitExceeded);
        }

        if let Some(location) = self.index.get(&id).copied() {
            let existing = self.read_location(id, location)?;
            if existing != bytes {
                return Err(CoreError::IdentityMismatch);
            }
            return Ok(PutOutcome::Reused);
        }

        let length = u32::try_from(bytes.len()).map_err(|_| CoreError::LengthOverflow)?;
        let offset = u64::try_from(self.payload.len()).map_err(|_| CoreError::LengthOverflow)?;
        let end = offset
            .checked_add(u64::from(length))
            .ok_or(CoreError::LengthOverflow)?;
        let stored_bytes = self
            .stored_bytes
            .checked_add(u64::from(length))
            .ok_or(CoreError::LengthOverflow)?;

        let capacity = self.payload.capacity();
        self.payload.extend_from_slice(bytes);
        if self.payload.capacity() != capacity {
            self.payload_reallocations = self
                .payload_reallocations
                .checked_add(1)
                .ok_or(CoreError::LengthOverflow)?;
            self.payload_growth_copy_estimate = self
                .payload_growth_copy_estimate
                .checked_add(offset)
                .ok_or(CoreError::LengthOverflow)?;
        }
        self.index.insert(id, ChunkLocation { offset, length });
        self.stored_bytes = stored_bytes;

        debug_assert_eq!(u64::try_from(self.payload.len()), Ok(end));
        Ok(PutOutcome::Inserted)
    }

    pub fn put_chunk(&mut self, bytes: &[u8]) -> CoreResult<(ChunkId, PutOutcome)> {
        if bytes.len() > MAXIMUM_CHUNK_BYTES {
            return Err(CoreError::ObjectLimitExceeded);
        }
        let id = chunk_id(bytes);
        let outcome = self.put_verified(id, bytes)?;
        Ok((id, outcome))
    }

    pub fn get(&self, id: ChunkId) -> CoreResult<&[u8]> {
        let location = self
            .index
            .get(&id)
            .copied()
            .ok_or(CoreError::MissingObject)?;
        self.read_location(id, location)
    }

    fn read_location(&self, id: ChunkId, location: ChunkLocation) -> CoreResult<&[u8]> {
        let start = usize::try_from(location.offset).map_err(|_| CoreError::LengthOverflow)?;
        let length = usize::try_from(location.length).map_err(|_| CoreError::LengthOverflow)?;
        let end = start.checked_add(length).ok_or(CoreError::LengthOverflow)?;
        let bytes = self
            .payload
            .get(start..end)
            .ok_or(CoreError::IdentityMismatch)?;
        if chunk_id(bytes) != id {
            return Err(CoreError::IdentityMismatch);
        }
        Ok(bytes)
    }

    pub fn object_count(&self) -> usize {
        self.index.len()
    }

    pub const fn stored_bytes(&self) -> u64 {
        self.stored_bytes
    }

    pub fn payload_len(&self) -> usize {
        self.payload.len()
    }

    pub fn payload_capacity(&self) -> usize {
        self.payload.capacity()
    }

    pub const fn payload_reallocations(&self) -> u64 {
        self.payload_reallocations
    }

    pub const fn payload_growth_copy_estimate(&self) -> u64 {
        self.payload_growth_copy_estimate
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;
    use crate::cdc::FastCdc;
    use crate::{chunk_id, ChunkId, ObjectId};

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
    fn chunk_identity_reuses_the_phase_one_object_domain() {
        let bytes = b"chunk bytes";
        assert_eq!(chunk_id(bytes), ObjectId::for_bytes(bytes));
        assert_eq!(ChunkId::for_bytes(bytes), chunk_id(bytes));
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
        let id = chunk_id(equal);
        let object_count = cas.object_count();
        assert_eq!(cas.put(id, equal), Ok(PutOutcome::Inserted));
        assert_eq!(cas.put(id, equal), Ok(PutOutcome::Reused));
        assert_eq!(cas.object_count(), object_count + 1);
    }

    #[test]
    fn rejects_missing_malformed_and_unequal_replacement() {
        let bytes = b"immutable";
        let id = chunk_id(bytes);
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
        assert!(cas.stored_bytes() <= u64::from(MAXIMUM_CHUNK_BYTES as u32) * 3);
    }

    #[test]
    fn packed_cas_inserts_and_reuses_without_appending() {
        let bytes = b"packed chunk";
        let id = chunk_id(bytes);
        let mut cas = PackedInMemoryCas::new();

        assert_eq!(cas.object_count(), 0);
        assert_eq!(cas.stored_bytes(), 0);
        assert_eq!(cas.put(id, bytes), Ok(PutOutcome::Inserted));
        assert_eq!(cas.object_count(), 1);
        assert_eq!(cas.stored_bytes(), bytes.len() as u64);
        assert_eq!(cas.payload.len(), bytes.len());
        assert_eq!(cas.get(id), Ok(bytes.as_slice()));

        assert_eq!(cas.put(id, bytes), Ok(PutOutcome::Reused));
        assert_eq!(cas.object_count(), 1);
        assert_eq!(cas.stored_bytes(), bytes.len() as u64);
        assert_eq!(cas.payload.len(), bytes.len());
    }

    #[test]
    fn packed_cas_rejects_wrong_id_without_mutating_state() {
        let bytes = b"packed chunk";
        let mut cas = PackedInMemoryCas::new();
        let before = (cas.object_count(), cas.stored_bytes(), cas.payload.len());

        assert_eq!(
            cas.put(chunk_id(b"different chunk"), bytes),
            Err(CoreError::IdentityMismatch)
        );
        assert_eq!(
            (cas.object_count(), cas.stored_bytes(), cas.payload.len()),
            before
        );
    }

    #[test]
    fn packed_cas_rejects_a_corrupt_incumbent() {
        let bytes = b"packed chunk";
        let id = chunk_id(bytes);
        let mut cas = PackedInMemoryCas::new();
        assert_eq!(cas.put(id, bytes), Ok(PutOutcome::Inserted));

        cas.payload[0] ^= 1;
        assert_eq!(cas.get(id), Err(CoreError::IdentityMismatch));
        assert_eq!(cas.put(id, bytes), Err(CoreError::IdentityMismatch));
        assert_eq!(cas.object_count(), 1);
        assert_eq!(cas.stored_bytes(), bytes.len() as u64);
    }

    #[test]
    fn packed_cas_rejects_oversized_chunks_and_missing_objects() {
        let mut cas = PackedInMemoryCas::new();
        let oversized = vec![0_u8; MAXIMUM_CHUNK_BYTES + 1];
        let oversized_id = chunk_id(&oversized);

        assert_eq!(
            cas.put_chunk(&oversized),
            Err(CoreError::ObjectLimitExceeded)
        );
        assert_eq!(
            cas.put(oversized_id, &oversized),
            Err(CoreError::ObjectLimitExceeded)
        );
        assert_eq!(cas.get(chunk_id(b"missing")), Err(CoreError::MissingObject));
        assert_eq!(cas.object_count(), 0);
        assert_eq!(cas.stored_bytes(), 0);
        assert!(cas.payload.is_empty());
    }

    #[test]
    fn packed_cas_reads_exact_indexed_ranges_and_fails_closed_on_bad_ranges() {
        let first = b"first packed chunk";
        let second = b"second packed chunk";
        let first_id = chunk_id(first);
        let second_id = chunk_id(second);
        let mut cas = PackedInMemoryCas::new();

        assert_eq!(cas.put(first_id, first), Ok(PutOutcome::Inserted));
        assert_eq!(cas.put(second_id, second), Ok(PutOutcome::Inserted));
        assert_eq!(cas.get(first_id), Ok(first.as_slice()));
        assert_eq!(cas.get(second_id), Ok(second.as_slice()));

        let payload_len = u64::try_from(cas.payload.len()).unwrap();
        {
            let location = cas.index.get_mut(&second_id).unwrap();
            location.offset = payload_len + 1;
        }
        assert_eq!(cas.get(second_id), Err(CoreError::IdentityMismatch));

        {
            let location = cas.index.get_mut(&second_id).unwrap();
            location.offset = u64::MAX;
        }
        assert_eq!(cas.get(second_id), Err(CoreError::LengthOverflow));
    }

    #[test]
    fn packed_cas_matches_in_memory_cas_for_deterministic_chunks() {
        let data = input(100_000);
        let mut plain = InMemoryCas::new();
        let mut packed = PackedInMemoryCas::new();
        let mut ids = Vec::new();

        FastCdc::new()
            .scan(Cursor::new(data.clone()), |chunk| {
                let (plain_id, plain_outcome) = plain.put_chunk(chunk)?;
                let (packed_id, packed_outcome) = packed.put_chunk(chunk)?;
                assert_eq!(packed_id, plain_id);
                assert_eq!(packed_outcome, plain_outcome);
                ids.push(plain_id);
                Ok(())
            })
            .unwrap();

        assert_eq!(packed.object_count(), plain.object_count());
        assert_eq!(packed.stored_bytes(), plain.stored_bytes());
        for id in ids {
            assert_eq!(packed.get(id), plain.get(id));
        }

        FastCdc::new()
            .scan(Cursor::new(data), |chunk| {
                assert_eq!(packed.put_chunk(chunk)?.1, PutOutcome::Reused);
                assert_eq!(plain.put_chunk(chunk)?.1, PutOutcome::Reused);
                Ok(())
            })
            .unwrap();
        assert_eq!(packed.stored_bytes(), plain.stored_bytes());
    }
}
