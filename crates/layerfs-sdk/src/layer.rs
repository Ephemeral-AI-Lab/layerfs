use crate::{LayerConnection, LayerConnectionId, StoreLocation};
use layerfs_layer_store::LayerStore;
use std::sync::Arc;

pub(crate) fn create(location: StoreLocation) -> layerfs_storage::Result<LayerConnection> {
    Ok(connection(
        location.clone(),
        LayerStore::create(location.path())?,
    ))
}

pub(crate) fn connect(location: StoreLocation) -> layerfs_storage::Result<LayerConnection> {
    Ok(connection(
        location.clone(),
        LayerStore::connect(location.path())?,
    ))
}

fn connection(location: StoreLocation, store: LayerStore) -> LayerConnection {
    LayerConnection {
        id: LayerConnectionId(crate::connection::id(b"layer", &location)),
        location,
        store: Arc::new(store),
    }
}
