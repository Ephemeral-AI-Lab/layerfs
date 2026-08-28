use crate::AuthenticatedSession;
use layerfs_sync::LocalDurable;
use layerfs_sync::{Direction, DurableEndpoint, RequestId};

pub(super) fn abort_transfer(
    session: &AuthenticatedSession<'_>,
    owner_request_id: RequestId,
    direction: Direction,
) -> layerfs_sync::Result<u64> {
    LocalDurable::new(session.durable()).abort_transfer(owner_request_id, direction)
}
