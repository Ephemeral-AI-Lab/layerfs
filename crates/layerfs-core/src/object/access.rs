use std::ops::Range;

use crate::{decode_bytes_object, CoreError, CoreResult, ObjectId};

pub trait ObjectRead {
    fn get(&self, id: ObjectId) -> CoreResult<Vec<u8>>;

    fn with_authenticated_canonical<T, F>(&self, id: ObjectId, callback: F) -> CoreResult<T>
    where
        F: FnOnce(&[u8]) -> CoreResult<T>,
    {
        let bytes = self.get(id)?;
        if ObjectId::for_bytes(&bytes) != id {
            return Err(CoreError::IdentityMismatch);
        }
        callback(&bytes)
    }

    fn get_authenticated_batch<F>(&self, ids: &[ObjectId], mut callback: F) -> CoreResult<()>
    where
        F: FnMut(ObjectId, &[u8]) -> CoreResult<()>,
    {
        for id in ids {
            self.with_authenticated_canonical(*id, |canonical| {
                callback(*id, decode_bytes_object(canonical)?)
            })?;
        }
        Ok(())
    }

    fn get_authenticated_payload_lengths_batch<F>(
        &self,
        ids: &[ObjectId],
        mut callback: F,
    ) -> CoreResult<()>
    where
        F: FnMut(ObjectId, u32) -> CoreResult<()>,
    {
        self.get_authenticated_batch(ids, |id, payload| {
            callback(
                id,
                u32::try_from(payload.len()).map_err(|_| CoreError::LengthOverflow)?,
            )
        })
    }

    fn get_authenticated_payload_ranges_batch<F>(
        &self,
        requests: &[(ObjectId, Range<u64>)],
        maximum_payload_len: u64,
        mut callback: F,
    ) -> CoreResult<()>
    where
        F: FnMut(ObjectId, &[u8]) -> CoreResult<()>,
    {
        let ids = requests.iter().map(|(id, _)| *id).collect::<Vec<_>>();
        let mut index = 0;
        self.get_authenticated_batch(&ids, |id, payload| {
            let (expected_id, range) = requests
                .get(index)
                .ok_or(CoreError::InvalidRecord("payload batch cardinality"))?;
            if id != *expected_id {
                return Err(CoreError::IdentityMismatch);
            }
            index += 1;
            let length = payload.len() as u64;
            if length > maximum_payload_len {
                return Err(CoreError::ChunkLengthMismatch);
            }
            if range.start > range.end || range.end > length {
                return Err(CoreError::InvalidRange {
                    start: range.start,
                    end: range.end,
                    length,
                });
            }
            let start = usize::try_from(range.start).map_err(|_| CoreError::LengthOverflow)?;
            let end = usize::try_from(range.end).map_err(|_| CoreError::LengthOverflow)?;
            callback(id, &payload[start..end])
        })?;
        if index != requests.len() {
            return Err(CoreError::InvalidRecord("payload batch cardinality"));
        }
        Ok(())
    }
}

pub trait ObjectStore {
    fn get(&self, id: ObjectId) -> CoreResult<Vec<u8>>;
    fn put(&mut self, canonical: &[u8]) -> CoreResult<ObjectId>;

    fn with_authenticated_canonical<T, F>(&self, id: ObjectId, callback: F) -> CoreResult<T>
    where
        F: FnOnce(&[u8]) -> CoreResult<T>,
    {
        let bytes = self.get(id)?;
        if ObjectId::for_bytes(&bytes) != id {
            return Err(CoreError::IdentityMismatch);
        }
        callback(&bytes)
    }
}

impl<T: ObjectStore> ObjectRead for T {
    fn get(&self, id: ObjectId) -> CoreResult<Vec<u8>> {
        ObjectStore::get(self, id)
    }

    fn with_authenticated_canonical<U, F>(&self, id: ObjectId, callback: F) -> CoreResult<U>
    where
        F: FnOnce(&[u8]) -> CoreResult<U>,
    {
        ObjectStore::with_authenticated_canonical(self, id, callback)
    }
}
