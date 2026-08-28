use layerfs_core::content::rope::ObjectStore;
use layerfs_core::inode::{
    diff_inode_table_entries, generated_inode_table_from_root, generated_inode_table_upsert,
    inode_table_lookup, inode_table_remove, inode_table_upsert, visit_inode_table_entries, InodeId,
    InodeTableCounters, InodeTableRoot,
};
use layerfs_core::namespace::{
    diff_directory_entries, directory_insert, directory_lookup, directory_remove, directory_rename,
    empty_directory, visit_directory_entries, DirectoryStateRoot, DirectoryStateV1,
    NamespaceCounters,
};
use layerfs_core::namespace_codec::{
    decode_directory_node, decode_directory_state, decode_inode_table_node, encode_directory_node,
    encode_directory_state, encode_inode_table_node, profile_id, DirectoryNodeV1, InodeTableNodeV1,
};
use layerfs_core::{CanonicalName, CoreError, CoreResult, ObjectId};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Default)]
struct MemoryStore(BTreeMap<ObjectId, Vec<u8>>);

impl ObjectStore for MemoryStore {
    fn get(&self, id: ObjectId) -> CoreResult<Vec<u8>> {
        self.0.get(&id).cloned().ok_or(CoreError::MissingObject)
    }
    fn put(&mut self, bytes: &[u8]) -> CoreResult<ObjectId> {
        let id = ObjectId::for_bytes(bytes);
        self.0.entry(id).or_insert_with(|| bytes.to_vec());
        Ok(id)
    }
}

fn new_ids(store: &MemoryStore, before: &BTreeSet<ObjectId>) -> BTreeSet<ObjectId> {
    store
        .0
        .keys()
        .filter(|id| !before.contains(id))
        .copied()
        .collect()
}

fn directory_reachable(store: &MemoryStore, root: DirectoryStateRoot) -> BTreeSet<ObjectId> {
    fn visit(store: &MemoryStore, id: ObjectId, reachable: &mut BTreeSet<ObjectId>) {
        if !reachable.insert(id) {
            return;
        }
        if let DirectoryNodeV1::Branch { children, .. } =
            decode_directory_node(store.0.get(&id).unwrap()).unwrap()
        {
            for (_, child) in children {
                visit(store, child, reachable);
            }
        }
    }
    let mut reachable = BTreeSet::from([root.0]);
    let state = decode_directory_state(store.0.get(&root.0).unwrap()).unwrap();
    visit(store, state.mapping_root, &mut reachable);
    reachable
}

fn inode_reachable(
    store: &MemoryStore,
    root: layerfs_core::inode::InodeTableRoot,
) -> BTreeSet<ObjectId> {
    fn visit(store: &MemoryStore, id: ObjectId, reachable: &mut BTreeSet<ObjectId>) {
        if !reachable.insert(id) {
            return;
        }
        if let InodeTableNodeV1::Branch { children, .. } =
            decode_inode_table_node(store.0.get(&id).unwrap()).unwrap()
        {
            for (_, child) in children {
                visit(store, child, reachable);
            }
        }
    }
    let mut reachable = BTreeSet::new();
    visit(store, root.0, &mut reachable);
    reachable
}

fn directory_leaf_counts(store: &MemoryStore, root: DirectoryStateRoot) -> Vec<usize> {
    let state = decode_directory_state(store.0.get(&root.0).unwrap()).unwrap();
    let DirectoryNodeV1::Branch { children, .. } =
        decode_directory_node(store.0.get(&state.mapping_root).unwrap()).unwrap()
    else {
        panic!("fixture is not a directory branch")
    };
    children
        .into_iter()
        .map(
            |(_, id)| match decode_directory_node(store.0.get(&id).unwrap()).unwrap() {
                DirectoryNodeV1::Leaf { entries, .. } => entries.len(),
                _ => panic!("fixture child is not a leaf"),
            },
        )
        .collect()
}

fn inode_leaf_counts(store: &MemoryStore, root: layerfs_core::inode::InodeTableRoot) -> Vec<usize> {
    let InodeTableNodeV1::Branch { children, .. } =
        decode_inode_table_node(store.0.get(&root.0).unwrap()).unwrap()
    else {
        panic!("fixture is not an inode branch")
    };
    children
        .into_iter()
        .map(
            |(_, id)| match decode_inode_table_node(store.0.get(&id).unwrap()).unwrap() {
                InodeTableNodeV1::Leaf(entries) => entries.len(),
                _ => panic!("fixture child is not a leaf"),
            },
        )
        .collect()
}
