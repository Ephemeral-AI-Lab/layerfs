pub use layerfs_stack_store::RemoteEndpoint;

use layerfs_layer_store::LayerStore;
use layerfs_stack_store::StackStore;
use layerfs_storage_core::{
    AddLayerSource, AddResult, BranchId, CommitId, LayerHistoryId, LayerId, RefOutcome, Result,
    StackHistoryId, StackId,
};
use std::sync::Arc;

#[derive(Clone)]
pub(crate) enum LayerEndpoint {
    Embedded(Arc<LayerStore>),
    Remote(RemoteEndpoint),
}

impl LayerEndpoint {
    pub(crate) fn add_layer(
        &self,
        history: LayerHistoryId,
        source: AddLayerSource,
    ) -> Result<AddResult<LayerId>> {
        match self {
            Self::Embedded(store) => store.add_layer(history, source),
            Self::Remote(store) => store.add_layer(history, source),
        }
    }
}

#[derive(Clone)]
pub(crate) enum StackPublicationEndpoint {
    Embedded(Arc<StackStore>),
    Remote(RemoteEndpoint),
}

impl StackPublicationEndpoint {
    pub(crate) fn add_stack(
        &self,
        history: StackHistoryId,
        branch: BranchId,
        commit: CommitId,
    ) -> Result<AddResult<StackId>> {
        match self {
            Self::Embedded(store) => store.add_stack(history, branch, commit),
            Self::Remote(store) => store.add_stack(history, branch, commit),
        }
    }

    pub(crate) fn push_stack(&self, stack: StackId) -> Result<RefOutcome<StackId>> {
        match self {
            Self::Embedded(store) => store.push_stack(stack),
            Self::Remote(store) => store.push_stack(stack),
        }
    }
}
