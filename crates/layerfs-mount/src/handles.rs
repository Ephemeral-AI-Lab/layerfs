use layerfs_sdk::NodeId;

#[derive(Default)]
pub struct Handles;

impl Handles {
    pub const fn insert(&self, node: NodeId) -> u64 {
        node.0
    }

    pub const fn get(&self, handle: u64) -> Option<NodeId> {
        if handle == 0 {
            None
        } else {
            Some(NodeId(handle))
        }
    }

    pub const fn remove(&self, handle: u64) -> Option<NodeId> {
        self.get(handle)
    }
}
