use crate::connection::BranchParent;
use crate::{
    BranchConnection, BranchConnectionId, LayerConnection, StackConnection, StoreLocation,
};
use layerfs_branch_store::BranchStore;
use layerfs_storage::StoreEndpoint;
use std::sync::Arc;

pub(crate) enum Parent<'a> {
    Layer(&'a LayerConnection),
    Stack(&'a StackConnection),
}

impl Parent<'_> {
    fn endpoint(&self) -> Arc<dyn StoreEndpoint> {
        match self {
            Self::Layer(parent) => parent.store.clone(),
            Self::Stack(parent) => parent.store.clone(),
        }
    }

    fn id(&self) -> BranchParent {
        match self {
            Self::Layer(parent) => BranchParent::Layer(parent.id),
            Self::Stack(parent) => BranchParent::Stack(parent.id),
        }
    }
}

pub(crate) fn create(
    location: StoreLocation,
    parent: Parent<'_>,
) -> layerfs_storage::Result<BranchConnection> {
    connection(location, parent, |path, endpoint| {
        BranchStore::create(path, endpoint)
    })
}

pub(crate) fn connect(
    location: StoreLocation,
    parent: Parent<'_>,
) -> layerfs_storage::Result<BranchConnection> {
    connection(location, parent, |path, endpoint| {
        BranchStore::connect(path, endpoint)
    })
}

fn connection(
    location: StoreLocation,
    parent: Parent<'_>,
    open: impl FnOnce(&std::path::Path, Arc<dyn StoreEndpoint>) -> layerfs_storage::Result<BranchStore>,
) -> layerfs_storage::Result<BranchConnection> {
    let store = open(location.path(), parent.endpoint())?;
    Ok(BranchConnection {
        id: BranchConnectionId(crate::connection::id(b"branch", &location)),
        location,
        parent: parent.id(),
        store,
    })
}
