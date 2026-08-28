use crate::{Direction, RequestId, Result};
use layerfs_core::ObjectId;

pub trait DurableEndpoint {
    fn durable_storage_id(&self) -> [u8; 32];
    fn read_object(&self, id: ObjectId, maximum: usize) -> Result<Vec<u8>>;
    fn contains_object(&self, id: ObjectId) -> Result<bool>;
    fn accept_objects(
        &self,
        owner_request_id: RequestId,
        request_id: RequestId,
        direction: Direction,
        objects: &[(ObjectId, Vec<u8>)],
    ) -> Result<()>;
    fn abort_transfer(&self, owner_request_id: RequestId, direction: Direction) -> Result<u64>;
}
