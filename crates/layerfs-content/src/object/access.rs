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
            let payload = crate::file::extent_codec::decode_chunk_payload(payload)?;
            callback(
                id,
                u32::try_from(payload.len()).map_err(|_| CoreError::LengthOverflow)?,
            )
        })
    }
}

pub trait ObjectStore {
    fn get(&self, id: ObjectId) -> CoreResult<Vec<u8>>;
    fn put(&mut self, canonical: &[u8]) -> CoreResult<ObjectId>;

    fn put_owned(&mut self, canonical: Vec<u8>) -> CoreResult<ObjectId> {
        self.put(&canonical)
    }

    #[doc(hidden)]
    fn note_transient_owned_bytes(&mut self, _bytes: u64) -> CoreResult<()> {
        Ok(())
    }

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
