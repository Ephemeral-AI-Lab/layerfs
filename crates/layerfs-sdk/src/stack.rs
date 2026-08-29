use crate::{LayerConnection, StackConnection, StackConnectionId, StoreLocation};
use layerfs_stack_store::StackStore;
use std::sync::Arc;

pub(crate) fn create(
    location: StoreLocation,
    parent: &LayerConnection,
) -> layerfs_storage::Result<StackConnection> {
    connection(location, parent, |path| {
        StackStore::create(path, parent.store.clone())
    })
}

pub(crate) fn connect(
    location: StoreLocation,
    parent: &LayerConnection,
) -> layerfs_storage::Result<StackConnection> {
    connection(location, parent, |path| {
        StackStore::connect(path, parent.store.clone())
    })
}

fn connection(
    location: StoreLocation,
    parent: &LayerConnection,
    open: impl FnOnce(&std::path::Path) -> layerfs_storage::Result<StackStore>,
) -> layerfs_storage::Result<StackConnection> {
    let store = open(location.path())?;
    Ok(StackConnection {
        id: StackConnectionId(crate::connection::id(b"stack", &location)),
        location,
        parent: parent.id,
        store: Arc::new(store),
    })
}
