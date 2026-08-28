use crate::WorkingStore;
use layerfs_core::object::access::ObjectRead;

impl ObjectRead for WorkingStore {
    fn get(&self, id: layerfs_core::ObjectId) -> layerfs_core::CoreResult<Vec<u8>> {
        ObjectRead::get(&self.storage, id)
    }

    fn with_authenticated_canonical<T, F>(
        &self,
        id: layerfs_core::ObjectId,
        callback: F,
    ) -> layerfs_core::CoreResult<T>
    where
        F: FnOnce(&[u8]) -> layerfs_core::CoreResult<T>,
    {
        ObjectRead::with_authenticated_canonical(&self.storage, id, callback)
    }
}
