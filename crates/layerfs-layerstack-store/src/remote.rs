use crate::LayerStackStore;
use layerfs_storage::LayerStackEndpoint;
use std::sync::Arc;

pub fn endpoint(store: Arc<LayerStackStore>) -> Arc<dyn LayerStackEndpoint> {
    store
}
