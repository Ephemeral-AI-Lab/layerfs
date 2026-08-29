use crate::StoreLocation;
use layerfs_branch_store::BranchStore;
use layerfs_layer_store::LayerStore;
use layerfs_stack_store::StackStore;
use std::sync::Arc;

macro_rules! connection_id {
    ($name:ident) => {
        #[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub struct $name(pub(crate) [u8; 16]);
    };
}

connection_id!(LayerConnectionId);
connection_id!(StackConnectionId);
connection_id!(BranchConnectionId);

#[derive(Clone)]
pub struct LayerConnection {
    pub id: LayerConnectionId,
    pub location: StoreLocation,
    pub store: Arc<LayerStore>,
}

#[derive(Clone)]
pub struct StackConnection {
    pub id: StackConnectionId,
    pub location: StoreLocation,
    pub parent: LayerConnectionId,
    pub store: Arc<StackStore>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BranchParent {
    Layer(LayerConnectionId),
    Stack(StackConnectionId),
}

#[derive(Clone)]
pub struct BranchConnection {
    pub id: BranchConnectionId,
    pub location: StoreLocation,
    pub parent: BranchParent,
    pub store: BranchStore,
}

#[derive(Clone)]
pub struct ConnectionContext {
    pub layer: LayerConnection,
    pub stacks: Vec<StackConnection>,
    pub branches: Vec<BranchConnection>,
    pub active_stack: Option<StackConnectionId>,
    pub active_branch: Option<BranchConnectionId>,
}

pub(crate) fn id(role: &[u8], location: &StoreLocation) -> [u8; 16] {
    fn hash(seed: u64, bytes: impl Iterator<Item = u8>) -> u64 {
        bytes.fold(seed, |value, byte| {
            (value ^ u64::from(byte)).wrapping_mul(0x100000001b3)
        })
    }
    let bytes = role.iter().copied().chain([0]).chain(
        location
            .path()
            .as_os_str()
            .as_encoded_bytes()
            .iter()
            .copied(),
    );
    let left = hash(0xcbf29ce484222325, bytes.clone());
    let right = hash(0x84222325cbf29ce4, bytes);
    [left.to_be_bytes(), right.to_be_bytes()].concat()[..16]
        .try_into()
        .expect("fixed connection id")
}
