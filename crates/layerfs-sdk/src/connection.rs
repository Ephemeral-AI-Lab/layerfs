use layerfs_branch_store::BranchStore;
use layerfs_layerstack_store::LayerStackStore;
use layerfs_storage::{LayerStackEndpoint as EndpointContract, StoreId};
use std::sync::Arc;

#[derive(Clone)]
pub struct LayerStackEndpoint {
    store: Arc<LayerStackStore>,
}

impl LayerStackEndpoint {
    pub fn local(store: Arc<LayerStackStore>) -> Self {
        Self { store }
    }

    pub fn store_id(&self) -> StoreId {
        self.store.store_id()
    }

    pub(crate) fn store(&self) -> &LayerStackStore {
        &self.store
    }

    pub(crate) fn contract(&self) -> Arc<dyn EndpointContract> {
        self.store.clone()
    }
}

impl From<Arc<LayerStackStore>> for LayerStackEndpoint {
    fn from(value: Arc<LayerStackStore>) -> Self {
        Self::local(value)
    }
}

pub struct ConnectionContext {
    pub layerstack: LayerStackEndpoint,
    pub branches: BranchStore,
}
