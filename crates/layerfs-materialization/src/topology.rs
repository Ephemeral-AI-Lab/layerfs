//! Canonical topology key encoding.

pub(crate) fn topology_edge_key(
    child: layerfs_core::inode::InodeId,
    parent: layerfs_core::inode::InodeId,
    name: &[u8],
) -> Vec<u8> {
    let mut key = Vec::with_capacity(64 + name.len());
    key.extend_from_slice(child.as_bytes());
    key.extend_from_slice(parent.as_bytes());
    key.extend_from_slice(name);
    key
}
