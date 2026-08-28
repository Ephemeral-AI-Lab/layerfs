use crate::AuthenticatedSession;
use layerfs_core::ObjectId;
use layerfs_sync::LocalDurable;
use layerfs_sync::{Direction, DurableEndpoint, RequestId};

pub(super) fn read_object(
    session: &AuthenticatedSession<'_>,
    id: ObjectId,
    maximum: usize,
) -> layerfs_sync::Result<Vec<u8>> {
    LocalDurable::new(session.durable()).read_object(id, maximum)
}

pub(super) fn contains_object(
    session: &AuthenticatedSession<'_>,
    id: ObjectId,
) -> layerfs_sync::Result<bool> {
    LocalDurable::new(session.durable()).contains_object(id)
}

pub(super) fn accept_objects(
    session: &AuthenticatedSession<'_>,
    owner_request_id: RequestId,
    request_id: RequestId,
    direction: Direction,
    objects: &[(ObjectId, Vec<u8>)],
) -> layerfs_sync::Result<()> {
    LocalDurable::new(session.durable()).accept_objects(
        owner_request_id,
        request_id,
        direction,
        objects,
    )
}

impl DurableEndpoint for AuthenticatedSession<'_> {
    fn durable_storage_id(&self) -> [u8; 32] {
        self.storage_id()
    }

    fn read_object(&self, id: ObjectId, maximum: usize) -> layerfs_sync::Result<Vec<u8>> {
        read_object(self, id, maximum)
    }

    fn contains_object(&self, id: ObjectId) -> layerfs_sync::Result<bool> {
        contains_object(self, id)
    }

    fn accept_objects(
        &self,
        owner_request_id: RequestId,
        request_id: RequestId,
        direction: Direction,
        objects: &[(ObjectId, Vec<u8>)],
    ) -> layerfs_sync::Result<()> {
        accept_objects(self, owner_request_id, request_id, direction, objects)
    }

    fn abort_transfer(
        &self,
        owner_request_id: RequestId,
        direction: Direction,
    ) -> layerfs_sync::Result<u64> {
        super::pin::abort_transfer(self, owner_request_id, direction)
    }
}
