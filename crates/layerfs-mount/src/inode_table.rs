use layerfs_sdk::NodeId;

#[derive(Default)]
pub struct InodeTable;

impl InodeTable {
    pub const fn kernel(&self, node: NodeId) -> u64 {
        node.0
    }

    pub const fn node(&self, ino: u64) -> Option<NodeId> {
        if ino == 0 {
            None
        } else {
            Some(NodeId(ino))
        }
    }
}
