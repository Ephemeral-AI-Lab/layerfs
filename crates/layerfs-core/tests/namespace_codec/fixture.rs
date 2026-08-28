use layerfs_core::content::rope::ObjectStore;
use layerfs_core::inode::{InodeId, InodeKind, InodeRecordV1};
use layerfs_core::metadata::{
    build_metadata_tree, decode_apple_acl, encode_apple_acl, metadata_lookup,
    metadata_tree_entries, AppleAclEntryV1, AppleAclTag, MetadataEntryV1, MetadataKey,
    MetadataTreeBuilder, PortableMetadataV1,
};
use layerfs_core::namespace::{DirectoryStateV1, NamespaceRootV1, SymlinkStateV1};
use layerfs_core::namespace_codec::*;
use layerfs_core::{encode_bytes_object, CoreError, ObjectId};
use std::collections::BTreeMap;

#[derive(Default)]
struct MemoryStore(BTreeMap<ObjectId, Vec<u8>>);
impl ObjectStore for MemoryStore {
    fn get(&self, id: ObjectId) -> Result<Vec<u8>, CoreError> {
        self.0.get(&id).cloned().ok_or(CoreError::MissingObject)
    }
    fn put(&mut self, bytes: &[u8]) -> Result<ObjectId, CoreError> {
        let id = ObjectId::for_bytes(bytes);
        self.0.entry(id).or_insert_with(|| bytes.to_vec());
        Ok(id)
    }
}
